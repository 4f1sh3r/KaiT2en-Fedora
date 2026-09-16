Name: t2-services-common
Version: 0.1.0
Release: 1%{?dist}
Summary: Shared Apple T2 network and suspend integration
License: GPL-3.0-or-later
URL: https://github.com/kaiT2en/KaiT2en-Fedora
Source0: t2-services-%{version}.tar.gz
BuildRequires: make
BuildRequires: systemd-rpm-macros
BuildArch: noarch
Requires: NetworkManager
Requires: systemd
Requires: iproute
Requires: iputils
Requires: coreutils
Requires: bash

%description
Shared Apple T2 network and suspend integration. This package does not require any other T2 feature package.

%prep
%autosetup -n t2-services-%{version}

%build
# No compiled code in the common runtime package.

%check
bash -n t2-services/shared/integration/libexec/t2-ncm-sleep

%install
make -C t2-services/shared install PREFIX=/usr DESTDIR=%{buildroot} SYSTEMD_UNIT_DIR=%{_unitdir} LIBEXECDIR=%{_libexecdir}

%post -p /bin/bash
if ! source %{_libexecdir}/t2-services/package-actions; then
    echo '[t2-services] cannot load package error reporter; service activation skipped' >&2
else
    t2_run systemctl daemon-reload
    if [ "$1" -eq 1 ]; then t2_run systemctl preset t2-services-suspend.service; fi
    t2_run systemctl try-restart t2-services-suspend.service
    t2_summary
fi
exit 0

%preun -p /bin/bash
if [ "$1" -eq 0 ]; then
    if source %{_libexecdir}/t2-services/package-actions; then
        t2_run systemctl disable --now t2-services-suspend.service
        t2_summary
    else
        echo '[t2-services] cannot load package error reporter; service cleanup skipped' >&2
    fi
fi
exit 0

%files
%license LICENSE
%doc t2-services/README.md
%{_libexecdir}/t2-services/t2-ncm-sleep
%{_libexecdir}/t2-services/package-actions
%{_unitdir}/t2-services-suspend.service
%config(noreplace) /etc/NetworkManager/conf.d/10-t2-services.conf
%config(noreplace) %attr(0600,root,root) /etc/NetworkManager/system-connections/t2-ncm.nmconnection
