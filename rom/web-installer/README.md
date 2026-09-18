# OctoSense web installer

One page that flashes the ROM from Chrome or Edge over WebUSB (the fastboot
protocol implemented in the browser by `fastboot.mjs`, android-fastboot 1.1.3).
Sparse images go over in 50 MB pieces with per-piece retries, which is what
marginal USB links need.

Serve it from any HTTPS origin, or from localhost for the bench:

    scripts/make-manifest.py ~/rom-builds/<build> "OctoSense 2026-09-18"
    ln -s ~/rom-builds/<build>/*.img ~/rom-builds/<build>/manifest.json web-installer/
    python3 -m http.server 8321 --directory web-installer

Then open http://localhost:8321 in Chrome with the phone in bootloader mode.
Nothing else running may hold the USB device (kill any `fastboot` process).
