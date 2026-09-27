#!/bin/bash
set -euo pipefail
# systemd starts this inside its own mount namespace; all mounts die with it.
base=${OCTOSENSE_BUILD_ROOT:-/home/ubuntu/octosense-adr0001}
rootfs="$base/rootfs"
mount --make-rprivate /
mount --bind "$base/build" "$rootfs/build"
mount --bind "$base/exports" "$rootfs/exports"
mount -t proc proc "$rootfs/proc"
mount --rbind /dev "$rootfs/dev"
mount --make-rslave "$rootfs/dev"
mount --rbind /sys "$rootfs/sys"
mount --make-rslave "$rootfs/sys"
exec chroot "$rootfs" /usr/sbin/runuser -u ubuntu -- /bin/bash /exports/build-rom.sh "$@"
