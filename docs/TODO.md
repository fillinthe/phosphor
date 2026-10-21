# To do (asked 2026-10-21: "do 2, 5, 6, 7, 8, 9, shortest first")

Updated as each one moves. `[ ]` not started · `[~]` working on it · `[x]` done (with what was checked).

## Next round (2026-10-21, "keep going on the large stuff"), quickest first

- [ ] **Stereo audio pictures**: shaders' audio inputs get left and right (today mono), so XY scopes and stereo
      visualizers work; then ship px-stream's XY scopes.
- [ ] **Instanced vertex shaders**: draw a shape many times (`gl_InstanceID`), which unlocks MadMapper's particle and
      instancing effects (Transform Particles, instancing, particles).
- [ ] **Hand-picked lostjared effects**: render every one of the 2,700 (MIT) to a picture, sort out the blank /
      broken / near-duplicates automatically, look at the rest on contact sheets, ship the best few dozen.
- [ ] **Hap R recording** (BC7: Hap's best quality), and with it a real check of Hap R playback.

## First round (asked 2026-10-21)

- [~] **Crash reports sent automatically** (#5): the app side is done (below); **needs you:** a private endpoint
      on your server. Spec: accept `POST` with JSON `{"version", "os", "arch", "report"}` (all strings), answer
      2xx, keep it private. Then put its address in `ENDPOINT` (crates/phosphor/src/crash_report.rs), or tell me.
- [?] **Render thread** (#6): **measured first, and the reasons for it don't show up.** Holding off unless you
      see a stall in real use (tell me when and what you were doing).
      - Menus: a new bench test (`--quick menutest`) opens a real macOS menu. Rendering went on at ~57 fps the
        whole time it was open, so menus don't freeze the outputs.
      - File dialogs are already non-blocking.
      - The idle cost (26% of a core) is mostly the system's GPU work; Phosphor's own code is a few percent of it.
      - Doing it means moving the engine (346 call sites) and the output windows off egui: the biggest, riskiest
        change in the app, for no measured gain today.

- [~] **Virtual camera** (#2): **built; needs your Apple Developer account to switch on.**
      - Output › Virtual camera turns it on: the composition as a camera called "Phosphor" (1920×1080, 30 fps).
      - The camera is a Core Media I/O camera extension (`macos/PhosphorCamera`, built by
        `scripts/build-camera-extension.sh`, packed into the app by `bundle-mac.sh`). Phosphor installs it and
        feeds it frames.
      - Checked: both chips build; the bundle carries the extension; Phosphor asked macOS to install it, and macOS
        answered "Missing entitlement com.apple.developer.system-extension.install", shown in the panel. That's
        the step only your account can pass.
      - **Needs you:** the steps at the top of `scripts/build-camera-extension.sh` (team ID, two App IDs, two
        Developer ID provisioning profiles, sign, notarize). The frame sending can't be tried until then.

## Done

- [x] **Windows installer** (#7): `Phosphor-windows-setup.exe` (Inno Setup), built on GitHub with every push and
      attached to releases next to the zip. Installs for the current user (no admin prompt), Start menu shortcut,
      optional desktop shortcut, `.phosphor` projects open in Phosphor (and Phosphor now opens a project given
      on its command line), uninstaller; the built-in updater keeps working. Checked on GitHub's Windows machine:
      silent install, files and file association present, the installed copy ran the test project, uninstall
      removed it. **Not tried on a real PC by hand.**

- [x] **Phosphor for Raspberry Pi / Linux ARM** (#9): built on GitHub's ARM Linux machines with every push
      (`.github/workflows/linux.yml`), packaged as `Phosphor-linux-arm64.tar.gz` (program, FFmpeg libraries,
      shaders, a start script and a README), attached to releases when it built. It's the full app; as a player it
      uses Session › Follow another Phosphor › "Every time Phosphor starts (a player)". Checked on GitHub: engine
      tests pass on ARM Linux, and the packaged app opened the test project on a virtual display (software Vulkan,
      8.5 fps on the CPU-only renderer; a Pi 5's GPU is far faster). **Not run on a real Pi** (none here). Media
      must be at the same paths as on the Mac.

- [x] **Crash reports, app side** (#5): Report a Bug › "Send crash reports by themselves" (off unless chosen;
      greyed until the server exists). After a crash, the same diagnostics the bug window shows (version,
      system, project shape as counts, end of crash.log; no project content, no file names) are posted once,
      and the recovery banner says whether it went. Checked: a local server received the JSON POST (test).

- [x] **Older macOS** (#9): Phosphor now runs from **macOS 13 Ventura** (was 14), the same floor as VDMX. One
      setting (`scripts/min-macos`) for FFmpeg, Syphon and the app; FFmpeg and Syphon rebuilt for it; a test
      bundle has every binary marked 13.0, its "too new" check and the panel check pass, and the linker gave
      no "built for newer macOS" warnings. 11–12 aren't possible: the screen-capture library calls macOS 13
      functions (it would mean a build without screen and window capture). **Not run on a real macOS 13 Mac** (none
      here).

- [x] **Masters** (#5): an output can show a whole comp as its program (Outputs list and Studio › output:
      "Shows: Main mix / Comp …"). Make one comp per master, switch its "In the master mix" off, and pick it on the
      output: each output then has its own mix. Checked: two comps (red, green), output 2 showing the green one,
      main output red, output 2 green; with the green comp still in the master mix, the picker says so.

- [x] **Shader packs** (#5), as far as is sensible:
      - daitomanabe's 11 (MIT) were already in; **px-stream**: of darosh's own shaders (MIT), Glitch Wave ships.
        His XY scopes convert but need stereo audio pictures (Phosphor's is mono), so they'd only draw a diagonal.
      - The converter now renames words that newer GLSL reserves (`sample`, `filter`, `input`…), which older ISF
        shaders use as names. That fixed 3 of these, and helps anything you import. No change across the 406-shader
        corpus.
      - **lostjared/shaders** (MIT, 2,700 effects): checked, not shipped. No categories or ratings to pick from, and
        converting 2,700 at every launch would cost tens of seconds of CPU. Worth a hand-picked set if you want one.

- [x] **One Inspector** (#8): the Inspector shows whatever you selected last: a layer (with its clip, and its
      effects as before), a mapping slice or mask, or a Control Surface widget. A Layer / Slice / Widget switch on
      top when more than one is selected. Checked: a click on a slice in the Studio turned the Inspector to that
      slice's settings (screenshot). Widgets use the same path; not seen on screen yet.
