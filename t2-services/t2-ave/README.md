# t2-ave

The userspace AVE service, its protocol and the `t2bce_ave` kernel module live
here. The existing `t2remote` command and `kait2en-t2-remote.service` names remain
available to preserve installed configurations and suspend clients.

`daemon/` manages service sessions, `protocols/` owns the AVE start/stop messages,
and `integration/` contains the unit and the hook installed into the common
NCM suspend helper. NetworkManager and NCM binding belong to `../shared/`.

Build and install userspace with `make build` and `make install`. Use `PREFIX`
and `DESTDIR` for packaging. Building the kernel module requires matching kernel
headers and the BCE core's symbols. The Fedora DKMS stack stages the module from
`kernel/t2bce_ave/` alongside the core.
