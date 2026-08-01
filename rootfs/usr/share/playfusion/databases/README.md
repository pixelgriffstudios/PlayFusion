# PlayFusion game-title databases

The `*-titles.tsv` files are generated from the corresponding Redump DATs in
[libretro-database](https://github.com/libretro/libretro-database), licensed
under CC-BY-SA-4.0. Lookup keys are normalized executable serials, product
codes, Nintendo game IDs, or normalized title aliases. PlayFusion uses the
tables only to display human-readable game titles.

Cover art is fetched on demand from
[libretro-thumbnails](https://github.com/libretro-thumbnails) and cached by
game identifier. When the console is offline or artwork is unavailable, the
platform's generic icon remains in use. Supported catalogs cover PlayStation,
PlayStation 2, GameCube, Wii, Dreamcast, Saturn, Sega CD, and PC Engine CD.
