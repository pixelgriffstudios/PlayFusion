#define FUSE_USE_VERSION 31

#include <errno.h>
#include <fcntl.h>
#include <fuse3/fuse.h>
#include <linux/cdrom.h>
#include <linux/fs.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

#define MAX_TRACKS 100
#define TEXT_BUFFER_SIZE 16384

enum stream_mode {
    STREAM_RAW_CD,
    STREAM_BLOCK_IMAGE,
};

struct track_info {
    int number;
    int lba;
    int data;
    int mode;
};

static const char *raw_path = "/disc.bin";
static const char *cue_path = "/disc.cue";
static const char *iso_path = "/disc.iso";
static const char *meta_path = "/disc.meta";
static int disc_fd = -1;
static int64_t disc_frames = 0;
static int64_t block_size = 0;
static enum stream_mode mode = STREAM_RAW_CD;
static struct track_info tracks[MAX_TRACKS];
static int track_count = 0;
static char cue_text[TEXT_BUFFER_SIZE];
static char meta_text[TEXT_BUFFER_SIZE];
static pthread_mutex_t disc_lock = PTHREAD_MUTEX_INITIALIZER;

union raw_frame {
    struct cdrom_msf msf;
    unsigned char bytes[CD_FRAMESIZE_RAW];
};

static void lba_to_msf(int64_t lba, struct cdrom_msf *msf)
{
    int64_t address = lba + CD_MSF_OFFSET;

    memset(msf, 0, sizeof(*msf));
    msf->cdmsf_min0 = (unsigned char)(address / (CD_SECS * CD_FRAMES));
    msf->cdmsf_sec0 = (unsigned char)((address / CD_FRAMES) % CD_SECS);
    msf->cdmsf_frame0 = (unsigned char)(address % CD_FRAMES);
}

static void lba_to_cue_time(int64_t lba, int *minutes, int *seconds, int *frames)
{
    *minutes = (int)(lba / (CD_SECS * CD_FRAMES));
    *seconds = (int)((lba / CD_FRAMES) % CD_SECS);
    *frames = (int)(lba % CD_FRAMES);
}

static int append_text(char *destination, size_t capacity, size_t *used,
                       const char *format, ...)
{
    va_list arguments;
    int written;

    if (*used >= capacity)
        return -1;
    va_start(arguments, format);
    written = vsnprintf(destination + *used, capacity - *used, format, arguments);
    va_end(arguments);
    if (written < 0 || (size_t)written >= capacity - *used)
        return -1;
    *used += (size_t)written;
    return 0;
}

static int path_is_regular_file(const char *path)
{
    if (mode == STREAM_RAW_CD)
        return strcmp(path, raw_path) == 0 ||
               strcmp(path, cue_path) == 0 ||
               strcmp(path, meta_path) == 0;
    return strcmp(path, iso_path) == 0 ||
           strcmp(path, meta_path) == 0;
}

static int stream_getattr(const char *path, struct stat *st,
                          struct fuse_file_info *fi)
{
    (void)fi;
    memset(st, 0, sizeof(*st));

    if (strcmp(path, "/") == 0) {
        st->st_mode = S_IFDIR | 0555;
        st->st_nlink = 2;
        return 0;
    }
    if (!path_is_regular_file(path))
        return -ENOENT;

    st->st_mode = S_IFREG | 0444;
    st->st_nlink = 1;
    if (strcmp(path, raw_path) == 0) {
        st->st_size = disc_frames * CD_FRAMESIZE_RAW;
        st->st_blksize = CD_FRAMESIZE_RAW;
    } else if (strcmp(path, iso_path) == 0) {
        st->st_size = block_size;
        st->st_blksize = 2048;
    } else if (strcmp(path, cue_path) == 0) {
        st->st_size = (off_t)strlen(cue_text);
        st->st_blksize = 4096;
    } else {
        st->st_size = (off_t)strlen(meta_text);
        st->st_blksize = 4096;
    }
    st->st_blocks = (st->st_size + 511) / 512;
    return 0;
}

static int stream_readdir(const char *path, void *buf, fuse_fill_dir_t filler,
                          off_t offset, struct fuse_file_info *fi,
                          enum fuse_readdir_flags flags)
{
    (void)offset;
    (void)fi;
    (void)flags;

    if (strcmp(path, "/") != 0)
        return -ENOENT;

    filler(buf, ".", NULL, 0, 0);
    filler(buf, "..", NULL, 0, 0);
    if (mode == STREAM_RAW_CD) {
        filler(buf, raw_path + 1, NULL, 0, 0);
        filler(buf, cue_path + 1, NULL, 0, 0);
    } else {
        filler(buf, iso_path + 1, NULL, 0, 0);
    }
    filler(buf, meta_path + 1, NULL, 0, 0);
    return 0;
}

static int stream_open(const char *path, struct fuse_file_info *fi)
{
    if (!path_is_regular_file(path))
        return -ENOENT;
    if ((fi->flags & O_ACCMODE) != O_RDONLY)
        return -EACCES;
    /*
     * Let the kernel cache and read ahead from the virtual image. A CD drive
     * may spend seconds retrying a damaged sector, so issuing the same raw
     * sector ioctl every time an emulator revisits data makes scratched discs
     * stutter much more than necessary. Each disc gets a fresh FUSE mount,
     * which prevents cached data from leaking between media changes.
     */
    fi->direct_io = 0;
    fi->keep_cache = 1;
    return 0;
}

static int read_text_file(const char *text_value, char *buf, size_t size,
                          off_t offset)
{
    size_t length = strlen(text_value);

    if (offset < 0)
        return -EINVAL;
    if ((size_t)offset >= length)
        return 0;
    if (size > length - (size_t)offset)
        size = length - (size_t)offset;
    memcpy(buf, text_value + offset, size);
    return (int)size;
}

static int read_raw_cd(char *buf, size_t size, off_t offset)
{
    int64_t file_size = disc_frames * CD_FRAMESIZE_RAW;
    size_t completed = 0;

    if (offset < 0)
        return -EINVAL;
    if (offset >= file_size)
        return 0;
    if ((int64_t)size > file_size - offset)
        size = (size_t)(file_size - offset);

    pthread_mutex_lock(&disc_lock);
    while (completed < size) {
        int64_t absolute = offset + (off_t)completed;
        int64_t frame_number = absolute / CD_FRAMESIZE_RAW;
        size_t within_frame = (size_t)(absolute % CD_FRAMESIZE_RAW);
        size_t available = CD_FRAMESIZE_RAW - within_frame;
        size_t wanted = size - completed < available ? size - completed : available;
        union raw_frame frame;

        memset(&frame, 0, sizeof(frame));
        lba_to_msf(frame_number, &frame.msf);
        if (ioctl(disc_fd, CDROMREADRAW, &frame) < 0) {
            int saved_errno = errno;
            pthread_mutex_unlock(&disc_lock);
            return completed > 0 ? (int)completed : -saved_errno;
        }

        memcpy(buf + completed, frame.bytes + within_frame, wanted);
        completed += wanted;
    }
    pthread_mutex_unlock(&disc_lock);
    return (int)completed;
}

static int read_block_image(char *buf, size_t size, off_t offset)
{
    ssize_t result;

    if (offset < 0)
        return -EINVAL;
    if (offset >= block_size)
        return 0;
    if ((int64_t)size > block_size - offset)
        size = (size_t)(block_size - offset);

    pthread_mutex_lock(&disc_lock);
    result = pread(disc_fd, buf, size, offset);
    if (result < 0)
        result = -errno;
    pthread_mutex_unlock(&disc_lock);
    return (int)result;
}

static int stream_read(const char *path, char *buf, size_t size, off_t offset,
                       struct fuse_file_info *fi)
{
    (void)fi;
    if (strcmp(path, cue_path) == 0 && mode == STREAM_RAW_CD)
        return read_text_file(cue_text, buf, size, offset);
    if (strcmp(path, meta_path) == 0)
        return read_text_file(meta_text, buf, size, offset);
    if (strcmp(path, raw_path) == 0 && mode == STREAM_RAW_CD)
        return read_raw_cd(buf, size, offset);
    if (strcmp(path, iso_path) == 0 && mode == STREAM_BLOCK_IMAGE)
        return read_block_image(buf, size, offset);
    return -ENOENT;
}

static void stream_destroy(void *private_data)
{
    (void)private_data;
    if (disc_fd >= 0) {
        ioctl(disc_fd, CDROM_LOCKDOOR, 0);
        close(disc_fd);
        disc_fd = -1;
    }
}

static const struct fuse_operations stream_operations = {
    .getattr = stream_getattr,
    .readdir = stream_readdir,
    .open = stream_open,
    .read = stream_read,
    .destroy = stream_destroy,
};

static int read_cd_layout(void)
{
    struct cdrom_tochdr header;
    struct cdrom_tocentry leadout;
    size_t cue_used = 0;
    size_t meta_used = 0;

    memset(&header, 0, sizeof(header));
    if (ioctl(disc_fd, CDROMREADTOCHDR, &header) < 0)
        return -1;

    memset(&leadout, 0, sizeof(leadout));
    leadout.cdte_track = CDROM_LEADOUT;
    leadout.cdte_format = CDROM_LBA;
    if (ioctl(disc_fd, CDROMREADTOCENTRY, &leadout) < 0)
        return -1;
    if (leadout.cdte_addr.lba <= 0)
        return -1;

    disc_frames = leadout.cdte_addr.lba;
    track_count = 0;
    for (int track = header.cdth_trk0;
         track <= header.cdth_trk1 && track_count < MAX_TRACKS; ++track) {
        struct cdrom_tocentry entry;

        memset(&entry, 0, sizeof(entry));
        entry.cdte_track = (unsigned char)track;
        entry.cdte_format = CDROM_LBA;
        if (ioctl(disc_fd, CDROMREADTOCENTRY, &entry) < 0)
            return -1;
        tracks[track_count].number = track;
        tracks[track_count].lba = entry.cdte_addr.lba;
        tracks[track_count].data = (entry.cdte_ctrl & CDROM_DATA_TRACK) != 0;
        tracks[track_count].mode = entry.cdte_datamode;
        track_count++;
    }
    if (track_count == 0)
        return -1;

    if (append_text(cue_text, sizeof(cue_text), &cue_used,
                    "FILE \"/run/kazeta/cdstream/disc.bin\" BINARY\n") < 0 ||
        append_text(meta_text, sizeof(meta_text), &meta_used,
                    "MODE=raw-cd\nFRAMES=%lld\nBYTES=%lld\n",
                    (long long)disc_frames,
                    (long long)(disc_frames * CD_FRAMESIZE_RAW)) < 0)
        return -1;

    for (int i = 0; i < track_count; ++i) {
        const char *type;
        int minutes;
        int seconds;
        int frames;

        if (!tracks[i].data)
            type = "AUDIO";
        else if (tracks[i].mode == 2)
            type = "MODE2/2352";
        else
            type = "MODE1/2352";
        lba_to_cue_time(tracks[i].lba, &minutes, &seconds, &frames);
        if (append_text(cue_text, sizeof(cue_text), &cue_used,
                        "  TRACK %02d %s\n"
                        "    INDEX 01 %02d:%02d:%02d\n",
                        tracks[i].number, type, minutes, seconds, frames) < 0 ||
            append_text(meta_text, sizeof(meta_text), &meta_used,
                        "TRACK=%02d\tLBA=%d\tTYPE=%s\tMODE=%d\n",
                        tracks[i].number, tracks[i].lba, type,
                        tracks[i].mode) < 0)
            return -1;
    }
    mode = STREAM_RAW_CD;
    return 0;
}

static int read_block_layout(void)
{
    uint64_t bytes = 0;
    off_t end;

    if (ioctl(disc_fd, BLKGETSIZE64, &bytes) < 0 || bytes == 0) {
        end = lseek(disc_fd, 0, SEEK_END);
        if (end <= 0)
            return -1;
        bytes = (uint64_t)end;
    }
    block_size = (int64_t)bytes;
    mode = STREAM_BLOCK_IMAGE;
    snprintf(meta_text, sizeof(meta_text),
             "MODE=block-image\nBYTES=%lld\nSECTOR_SIZE=2048\n",
             (long long)block_size);
    return 0;
}

int main(int argc, char **argv)
{
    char **fuse_argv;
    int fuse_argc;
    int result;

    if (argc < 3) {
        fprintf(stderr, "Usage: %s DEVICE MOUNTPOINT [FUSE options]\n", argv[0]);
        return 2;
    }

    disc_fd = open(argv[1], O_RDONLY | O_NONBLOCK);
    if (disc_fd < 0) {
        perror(argv[1]);
        return 1;
    }
    /*
     * Some drives report a synthetic one-track CD TOC for DVDs. Console DVD
     * images are larger than any supported CD, so prefer block streaming when
     * BLKGETSIZE64 reports at least one billion bytes.
     */
    if (read_block_layout() == 0 && block_size >= 1000000000LL) {
        mode = STREAM_BLOCK_IMAGE;
    } else if (read_cd_layout() < 0 && read_block_layout() < 0) {
        fprintf(stderr, "Unable to read CD TOC or block image size from %s\n", argv[1]);
        close(disc_fd);
        return 1;
    }

    ioctl(disc_fd, CDROM_LOCKDOOR, 1);
    if (mode == STREAM_RAW_CD) {
        fprintf(stderr, "Streaming %lld raw CD frames (%lld bytes, %d tracks) from %s\n",
                (long long)disc_frames,
                (long long)(disc_frames * CD_FRAMESIZE_RAW),
                track_count, argv[1]);
    } else {
        fprintf(stderr, "Streaming %lld-byte block image from %s\n",
                (long long)block_size, argv[1]);
    }

    fuse_argc = argc - 1;
    fuse_argv = calloc((size_t)fuse_argc + 1, sizeof(*fuse_argv));
    if (!fuse_argv) {
        perror("calloc");
        close(disc_fd);
        return 1;
    }
    fuse_argv[0] = argv[0];
    for (int i = 2; i < argc; ++i)
        fuse_argv[i - 1] = argv[i];

    result = fuse_main(fuse_argc, fuse_argv, &stream_operations, NULL);
    free(fuse_argv);
    return result;
}
