# To do (asked 2026-10-21: "do 2, 5, 6, 7, 8, 9, shortest first")

Updated as each one moves. `[ ]` not started · `[~]` working on it · `[x]` done (with what was checked).

## Quickest first

- [~] **Windows installer** (#7) — built and pushed; waiting for the Windows build to install / run / uninstall it.: a Setup.exe built on GitHub with every push; CI installs it silently on the
      Windows machine and checks the installed Phosphor starts. Zip stays for people who prefer it.
- [ ] **Shader packs** (#5): download the free packs on the list (MIT / CC0 only, licence checked per file),
      convert them, ship the ones that work.
- [ ] **Older macOS 11–13** (#9): build for macOS 11, check what breaks (FFmpeg build target, APIs used).
- [ ] **Phosphor Player for Raspberry Pi** (#9): a Linux ARM build that follows a Mac (Follow another Phosphor) on
      a Pi, built on GitHub.
- [ ] **Masters** (#5): several program mixes, each with its own layers' mix, for different outputs.
- [ ] **Crash reports sent automatically** (#5): needs a private endpoint on a server you run;
      I'll build the app side and say what the server needs.
- [ ] **Render thread** (#6): rendering on its own thread, so the UI never slows the outputs (and costs less when
      idle).
- [ ] **Virtual camera** (#2): Phosphor as a webcam in Zoom / OBS / Teams. The camera extension can be built here,
      but macOS only loads it when it's signed with your Developer ID: it can't be tried until you sign it.

## Done

- [x] **One Inspector** (#8): the Inspector shows whatever you selected last: a layer (with its clip, and its
      effects as before), a mapping slice or mask, or a Control Surface widget. A Layer / Slice / Widget switch on
      top when more than one is selected. Checked: a click on a slice in the Studio turned the Inspector to that
      slice's settings (screenshot). Widgets use the same path; not seen on screen yet.
