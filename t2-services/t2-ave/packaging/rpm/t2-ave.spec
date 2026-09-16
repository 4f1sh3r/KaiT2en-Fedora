Name: t2-ave
Version: 0.1.0
Release: 1%{?dist}
Summary: Apple T2 AVE service
License: GPL-3.0-or-later
URL: https://github.com/kaiT2en/KaiT2en-Fedora
Source0: t2-services-%{version}.tar.gz
BuildRequires: make
BuildRequires: systemd-rpm-macros
BuildRequires: cargo
BuildRequires: rust
BuildRequires: gcc
Requires: t2-services-common >= 0.1.0
Requires: kmod

%description
Apple T2 AVE service. This package does not require any other T2 feature package.
A compatible t2bce_ave kernel module must be supplied by the distribution.

%prep
%autosetup -n t2-services-%{version}

%build
make -C t2-services/t2-ave build CARGO_FLAGS='--frozen'

%check
make -C t2-services/t2-ave test CARGO_FLAGS='--frozen'

%install
make -C t2-services/t2-ave install PREFIX=/usr DESTDIR=%{buildroot} SYSTEMD_UNIT_DIR=%{_unitdir} LIBEXECDIR=%{_libexecdir}

%post -p /bin/bash
if ! source %{_libexecdir}/t2-services/package-actions; then
    echo '[t2-services] cannot load package error reporter; service activation skipped' >&2
else
    t2_run systemctl daemon-reload
    if [ "$1" -eq 1 ]; then t2_run systemctl preset kait2en-t2-remote.service; fi
    t2_run systemctl try-restart kait2en-t2-remote.service
    t2_summary
fi
exit 0

%preun -p /bin/bash
if [ "$1" -eq 0 ]; then
    if source %{_libexecdir}/t2-services/package-actions; then
        t2_run systemctl disable --now kait2en-t2-remote.service
        t2_summary
    else
        echo '[t2-services] cannot load package error reporter; service cleanup skipped' >&2
    fi
fi
exit 0

%files
%license LICENSE
%doc t2-services/README.md
%{_bindir}/t2remote
%{_unitdir}/kait2en-t2-remote.service
%{_libexecdir}/t2-services/sleep.d/t2-ave
