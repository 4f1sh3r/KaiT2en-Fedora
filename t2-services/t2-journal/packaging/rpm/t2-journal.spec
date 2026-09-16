Name: t2-journal
Version: 0.1.0
Release: 1%{?dist}
Summary: Apple T2 bridgeOS and Linux journal reader
License: GPL-3.0-or-later AND Apache-2.0
URL: https://github.com/kaiT2en/KaiT2en-Fedora
Source0: t2-services-%{version}.tar.gz
BuildRequires: make
BuildRequires: systemd-rpm-macros
BuildRequires: cargo
BuildRequires: rust
BuildRequires: gcc
Requires: t2-services-common >= 0.1.0
Requires: systemd

%description
Apple T2 bridgeOS and Linux journal reader. This package does not require any other T2 feature package.

%prep
%autosetup -n t2-services-%{version}

%build
make -C t2-services/t2-journal build CARGO_FLAGS='--frozen'

%check
make -C t2-services/t2-journal test CARGO_FLAGS='--frozen'

%install
make -C t2-services/t2-journal install PREFIX=/usr DESTDIR=%{buildroot}

%files
%license LICENSE
%doc t2-services/README.md
%{_bindir}/t2journal
