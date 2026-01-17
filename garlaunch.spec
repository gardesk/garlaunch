Name:           garlaunch
Version:        0.1.0
Release:        1%{?dist}
Summary:        Application launcher for the gar desktop suite

License:        MIT
URL:            https://github.com/gardesk/garlaunch
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.75
BuildRequires:  cargo
BuildRequires:  libxcb-devel
BuildRequires:  cairo-devel
BuildRequires:  pango-devel

# Disable debug package
%global debug_package %{nil}

%description
Garlaunch is a rofi-like application launcher for X11 built in Rust. Features
fuzzy search, .desktop file parsing, and a sleek UI. Part of the gardesk
desktop environment suite.

%prep
%autosetup

%build
export CARGO_TARGET_DIR=target
cargo build --release --workspace

%install
install -Dm755 target/release/garlaunch %{buildroot}%{_bindir}/garlaunch
install -Dm755 target/release/garlaunchctl %{buildroot}%{_bindir}/garlaunchctl

%files
%{_bindir}/garlaunch
%{_bindir}/garlaunchctl

%changelog
* Fri Jan 17 2025 mfw <espadonne@outlook.com> - 0.1.0-1
- Initial RPM release of garlaunch
- Application launcher with fuzzy search
- Part of gardesk desktop suite
