# PDF Tools on Linux — 2026-10-11

English | [简体中文](README.zh-CN.md)

Until now PDF Tools had been verified only on macOS. Here [ui.py](../../ui.py)
ran its four shell journeys on Linux, on main 5cab54ad, which has #479 (the
follow-ups) and #478 (the window hint). This branch changes only ui.py, which
now also launches the shell on Linux. During the runs main moved twice:
first to 3632d376, adding #485 (the dock hides while OctoSense is full
screen) and #494 (the pdfcraft repin), then to 3a563057, adding #487
(maximize and snaps end above the dock). After each move the branch was
rebased, and the journeys, the import and the window passed again; the
frames are from the first run, on 5cab54ad. [receipt.json](receipt.json)
holds the binaries' and the sources' digests, the commands, what each run
saw, the Linux findings and what was not verified.

**The setup:**
- The Linux build host: Ubuntu 26.04 LTS, x86_64, 72 CPUs, no GPU and no
  desktop session.
- A scratch checkout of main, prepared by `tools/setup.py` against the host's
  source hubs.
- The desktop shell, with the real `pdf` engine (`cargo build --locked -p
  octosense --no-default-features --features app-hub,craft-engines`, debug).
- On Linux, ui.py gives each session a headless weston of its own in a
  private runtime folder (`weston --backend=headless --renderer=pixman
  --idle-time=0 --socket=… --no-config`).
  - The shell runs on it in a session of its own, with Makepad's Wayland
    backend and the run home's XDG folders.
  - Mesa's llvmpipe draws (software GL).
  - The session ends with `/quit`, and weston stops after the shell.
  - `MAKEPAD_HIDE_WINDOWS` does nothing on Linux, so the window is mapped on
    that weston.
- Every home was on ext4 (`--work`): the host's `/tmp` is tmpfs.
- The runtime's 64 ms script budget throughout.
- Each frame is the app's area cut from a `/g` grab taken at one pixel per
  point (`--grab-scale 1`; the window's dpi is 1).

```sh
python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <work>/octosense-shell.lock \
    --work <work>/homes --grab-scale 1 --output <work>/ui-64ms
TZ=Pacific/Kiritimati python3 apps/pdftools/tests/ui.py --shell target/debug/octosense \
    --lock <work>/octosense-shell.lock --work <work>/homes --grab-scale 1 --output <work>/ui-tz
python3 apps/pdftools/tests/evidence/linux-20261011/import/check.py \
    --shell target/debug/octosense --output <work>/import-check
```

**The runs:**
- **In the host's own zone (UTC),** from 14:25 to 14:29 UTC at load averages
  of 18 to 35: `shell` (light, then dark), `restart`, `full` and `empty` all
  passed on the first attempt.
- **With `TZ=Pacific/Kiritimati` (UTC+14),** from 14:32 to 14:36 UTC, when
  it was already 12 Oct there: the same four passed again.
- **With the final ui.py, in UTC,** from 14:54 to 14:58 UTC, while another
  session's local CI ran on the host: the same four passed a third time.
  The first two runs and the import check used ui.py before its last
  change, which only stops the shell and its weston when the bridge never
  answers.
- **After the rebases onto main 3632d376, then 3a563057,** from 15:02 to
  15:06 and from 15:10 to 15:14 UTC: the same four passed a fourth and a
  fifth time, PDF Tools opened at the same sizes, and the import check
  passed again each time.
- No log had a script error or a budget overrun, and every shell exited
  after `/quit`.
- **[import/check.py](import/check.py)** ran "Open a PDF from this device"
  with a stand-in dialog helper: see below.
- The pdf service's tests (`cargo test --locked -p octosense-pdf-service`)
  and the PDF Tools model tests (`cargo test --locked -p octosense-shell --lib
  pdftools_model`, 18) pass on the host too.

**The frames:**
- [light/](light), [dark/](dark), [restart/](restart), [full/](full) and
  [empty/](empty) are from the first run. They show what the macOS frames
  in [v2-followups-20261010](../v2-followups-20261010/README.md) show,
  except for the file dialog.
  - **01-home, 01b-open-from-device:** "Not available here" beside the
    button, and the button's banner (below). The status line reads "187 KB
    of 64 MB used", as on the Mac.
  - **02c-read-mode, 02d-read-mode-zoomed, 02e-read-mode-left:** Read mode,
    entered on page 2 of 4. Then page 3 at 125%, and left on page 3 at 125%.
  - **03-find, 03b-find-done:** "9 matches on 4 pages" from the top bar.
    Done gives back the Comments panel.
  - **04-comment, 04b-replied:** the highlight and its reply, "Today 14:25"
    at 14:25 UTC.
  - **05-pages-rotated, 06-combine, 06b-combined, 07-fill-sign, 08a-colours,
    08b-edited, 09-saved, 10-damaged:** the rotation, Combine with its button
    in view, the initials, the paragraph centred and red, the save and the
    damaged file.
  - **dark/:** the same in the dark style. Read mode is left by its corner
    control, and the paragraph is set flush right.
  - **restart/:** the covers kept, the page a PDF was left at, the import,
    and a removal taking its cover.
  - **full/:** "63.9 of 64 MB used", and a removal.
  - **empty/:** light and dark.
- [compare/](compare): Linux on the left, macOS on the right, halved: Home, a
  page, find and the edited paragraph.
- [tz/](tz): the second run's highlight and reply. **04b-replied** reads
  "Today 04:32" at 14:32 UTC on 11 Oct, which was 04:32 on 12 Oct in UTC+14.
- [import/](import): the import through the stand-in helper.
- [window/opened-from-the-menu.png](window/opened-from-the-menu.png): the
  whole window, with PDF Tools opened from the shell's menu.

## What is different on Linux

- **The file dialog.** makepad's Linux file dialog is neither
  xdg-desktop-portal nor GTK. It runs the desktop's own helper, the first of
  zenity, qarma, matedialog and kdialog on `PATH` (kdialog first under KDE),
  and reads the chosen path from its output
  (`platform/src/os/linux/file_dialog.rs`).
  - The build host has none of these, so the shell does not offer
    `files.import`, and `files.status` answers `import_supported: false`.
  - PDF Tools shows "Not available here" beside "Open a PDF from this
    device". Its button shows the banner "This OctoSense can't open files
    from the device for PDF Tools." Nothing hangs, and nothing is logged as
    an error. On Linux, ui.py now checks this instead of the macOS refusal.
  - On the Mac a hidden window never has focus, so the dialog is refused.
    On Linux the window is mapped on the run's weston, and the files service
    counts a window outside macOS as focused until it reports losing focus.
    So with a helper on `PATH` the dialog would really open. ui.py leaves
    the step out on such a machine and says so in its run.json.
- **A real import, with a stand-in helper.** [import/check.py](import/check.py)
  puts a stand-in for zenity first on the shell's `PATH`. The stand-in
  records how it was asked and answers with a copy of the samples' field
  guide named `Lease 2026.pdf`. Everything after it is shipped code.
  - The helper was asked `zenity --file-selection --title=Import a file into
    this app`.
  - The 16,477 bytes were copied byte for byte into
    `accounts/device/library/Imported PDF.pdf`.
  - `library.json` titles it "Lease 2026", with 7 pages and a cover.
  - It opened in a tab, "Lease 2026.pdf", on "1 / 7"
    ([import.json](import/import.json)).
  - No test had run a real import before: on the Mac the hidden window has
    the dialog refused. The real helpers' own windows were not run.
  - The import passes no file filter (the files service adds one only for
    photos), so a helper would list every file. That is the same on every
    platform.
- **Fonts and text.** The host has only DejaVu (8 font files), and nothing
  differs from macOS. The shell's interface draws with Makepad's bundled
  fonts. The engine's renders of the samples' sans-serif and serif text
  match the Mac's: the same text, wrapping and layout in compare/. CJK text
  was not exercised.
- **Software GL.** A shell's start stalls its UI thread while llvmpipe
  compiles shaders: a first event of 3.1 to 3.6 s, and draw-list stalls of
  0.5 to 1.2 s (one more comes with the first page). Apart from those, no
  stall logged in the five runs was longer than 409 ms, and no script
  handler overran 64 ms.
- **The window.** On weston the shell's window is 1400 x 900 points at dpi 1.
  - At startup the log says "wm: pdftools asks for a 1536x1024 window; it
    opens at 1296x698 (the dock covers 78 points of the desk)". That is an
    app area of 1292 x 662, the wide layout, as in the macOS hidden shell.
  - Opened from the menu once the desk had drawn, the window is 1296 x 709
    (an app area of 1292 x 673) and ends above the dock. The macOS hidden
    shell, a 1400 x 897 window, gave 1296 x 703.
  - With #485 the dock hides only while OctoSense is full screen (on Wayland,
    also maximized). The shell's window on weston is neither, so the dock
    stays, and PDF Tools opened at the same sizes on main 3632d376 and on
    3a563057, which also has #487.
- **Comment dates.** In UTC the local time is UTC itself, so the first run
  alone cannot tell the two apart. Under UTC+14 the dates crossed midnight
  correctly: the highlight read "Today 04:32", not 14:32 and not
  "Yesterday".
  - The pdf service takes the offset from `localtime_r`.
  - The shell sets the script's `std.local_time` offset from the same call,
    so the two agree.
- **ext4.** `pdf.save`'s staging and rename, the render cache's writes and
  the covers all ran at 64 ms with no overrun, at load averages of 10 to 35.
  - On the Mac the 64 ms runs overran after 262 and 555 ms renames, at load
    averages of 54 to 74, so this is not a like-for-like comparison.
  - After the restart the five current covers kept their nanosecond
    modification times, and only the new import's cover was drawn.
  - Remove deleted the PDF's cover.
  - The cache evicts by modification time, not by directory order (which
    differs between ext4 and APFS).

No Linux-only bug turned up, so this branch carries no fix.

## Not verified

- Windows: there is no machine here.
- A real helper's own dialog (zenity, qarma, matedialog, kdialog). None is
  installed on the host, and nothing was installed for this; a stand-in
  answered. makepad has no xdg-desktop-portal path.
- X11: weston's Xwayland cannot start on the host (it has no
  `/tmp/.X11-unix`). Also a desktop compositor such as GNOME or KDE, a GPU
  driver, and a scale above 1 on Linux.
- ui.py's card-host runs (`missing`, `fixture`) on Linux, and a release
  build.
- CJK and other non-Latin text on a host with no such fonts.
