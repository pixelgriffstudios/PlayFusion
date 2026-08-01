# PlayFusion Movies and Music

PlayFusion scans removable SD cards and USB storage for common movie and music
files. Open **Extras > Movies** or **Extras > Music Library** to browse the
detected media alongside files already installed internally.

## Naming

Movie filenames should use a title and optional release year:

```text
The Hunger Games (2012).mp4
The Hunger Games (2012) 1080p.mkv
```

Music uses embedded tags when available. Untagged files can use:

```text
Artist Name - Song Name.mp3
Artist Name - Album Name/
  Song Name.wav
```

Movie art is matched from the film's Wikipedia page. Music art first uses
embedded artwork, then MusicBrainz and the Cover Art Archive. A nearby
`cover.jpg`, `folder.jpg`, `cover.png`, or `folder.png` takes priority.

## Controls

- **A**: play the selected movie or song
- **X**: install selected removable media internally
- **LB/RB**: previous/next gallery page
- **B**: return to Extras

Installed movies are stored under `/var/kazeta/movies`. Installed music is
stored under `/var/kazeta/music` and is also available in the MP3 Jukebox.
Both folders are exposed by PlayFusion's FTP service as `movies` and `music`.

Supported movie formats:

```text
MP4, MKV, AVI, MOV, M4V, WebM, MPG, MPEG, TS, M2TS
```

Supported music formats:

```text
MP3, WAV, FLAC, OGG, Opus, M4A, AAC, WMA
```
