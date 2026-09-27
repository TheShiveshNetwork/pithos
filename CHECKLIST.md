
[x] Add namespace

[x] Isolate filesystem

[x] Network isolation

[x] PID namespace fix: currently clone() child becomes PID 1 but PID 1 realistically has a special responsibility: reap orphaned / zombie processes

[x] Replace chroot with pivot_root for safety

[] Implement real rootfs

[] Add cgroups

[] Implment user namespaces

[] Use Linux Capabilities

[] seccomp

[] Implment read-only fs

[] Mask sensitive paths to make inaccessible to the container (e.g., /proc/kcore, /proc/keys)

[] Implement container configs

