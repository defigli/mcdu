Name:           mcdu
Version:        0.3.5
Release:        1%{?dist}
Summary:        Modern disk usage analyzer with terminal UI

License:        MIT
URL:            https://github.com/mikalv/mcdu
Source0:        https://github.com/mikalv/%{name}/archive/refs/tags/v%{version}.tar.gz

BuildRequires:  rust >= 1.70
BuildRequires:  cargo
BuildRequires:  gcc

%description
mcdu is a modern disk usage analyzer inspired by ncdu.
It provides a fast, interactive terminal interface for
exploring disk usage and safely deleting files.

Features:
- Fast directory scanning with progress display
- Instant navigation (full tree in memory)
- Safe file deletion with confirmation
- Size change detection between scans

%prep
%autosetup -n %{name}-%{version}

%build
cargo build --release --locked

%install
install -Dm755 target/release/%{name} %{buildroot}%{_bindir}/%{name}

%files
%license LICENSE
%{_bindir}/%{name}

%changelog
* Wed Jan 01 2025 Mikal Villa <m@meeh.dev> - 0.3.5-1
- Initial package
