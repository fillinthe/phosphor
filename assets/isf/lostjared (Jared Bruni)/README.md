# lostjared shaders (by Jared Bruni)

53 effects hand-picked from [lostjared/shaders](https://github.com/lostjared/shaders) (MIT, see
LICENSE-lostjared.txt), checked 2026-10-21. Each file's description ends with its original name there.

How they were picked: the collection's 2,793 fragment shaders were converted to ISF by a script (renamed inputs:
`samp` → `inputImage`, `time_f` / `iTime` → `TIME`, `iResolution` → `RENDERSIZE`, `tc` → the ISF coordinate; the
sound levels and spectrum come from an `audioFFT` input; the mouse is the middle of the picture). Shaders that need a
history of past frames, 3D matrices or settings of their own were left out. 1,330 of the rest convert and compile.
Each was drawn over a photo at three moments with a made-up beat. Blank ones, ones that leave the picture as it was,
ones that flash hard from one frame to the next, and near-duplicates were dropped automatically (773 left). The 53
here were chosen by eye from those, avoiding look-alikes.

The collection warns that some of its shaders flash or change colour fast. The hardest flashers were dropped, but
some of these still pulse with the music.
