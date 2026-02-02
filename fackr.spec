Name:           fackr
Version:        1.2.0
Release:        1%{?dist}
Summary:        Terminal text editor written in Rust

License:        MIT
URL:            https://github.com/tenseleyFlow/fackr

%description
fackr is a terminal text editor written in Rust, a reimplementation of
facsimile with VSCode-style keybindings. It provides a familiar editing
experience for developers who appreciate the power of keyboard shortcuts
combined with the efficiency of terminal-based workflows.

Features:
- VSCode-style keybindings (Ctrl+C/V/X, Ctrl+S, Ctrl+D, etc.)
- Rope-based text buffer for efficient editing
- Cross-platform clipboard support
- Fast and lightweight

%prep
# Pre-built from source

%build
# Pre-built from source

%install
mkdir -p %{buildroot}%{_bindir}
install -Dm755 %{_sourcedir}/fackr %{buildroot}%{_bindir}/fackr

%files
%{_bindir}/fackr

%changelog
* Tue Jan 28 2026 mfw <espadonne@outlook.com> - 1.2.0-1
- Version bump to 1.2.0
- Bug fixes and improvements

* Tue Jan 07 2025 mfw <espadonne@outlook.com> - 1.1.2-1
- Version bump to 1.1.2

* Wed Dec 11 2024 mfw <espadon@outlook.com> - 0.9.6-1
- Fix command palette char input
- Create new file from CLI

* Wed Dec 11 2024 mfw <espadon@outlook.com> - 0.9.5-1
- Command palette (Ctrl+P) with fuzzy search

* Wed Dec 11 2024 mfw <espadon@outlook.com> - 0.9.4-1
- Ctrl+D scrolls viewport to show newly added cursor

* Tue Dec 10 2024 mfw <espadon@outlook.com> - 0.9.0-1
- Add Ctrl+/ toggle line comment
- Performance optimizations

* Mon Dec 09 2024 mfw <espadon@outlook.com> - 0.8.0-1
- Add Ctrl+G/F5 goto line with line:col syntax
- Add Ctrl+O fortress file browser
- Fix multi-cursor same-line edits

* Mon Dec 09 2024 mfw <espadon@outlook.com> - 0.7.0-1
- Add Ctrl+F/Ctrl+R find and replace with regex support
- Fuzzy filter improvements and references panel visual fix

* Sun Dec 08 2024 mfw <espadon@outlook.com> - 0.6.0-1
- Add Shift+F12 references panel with filtering
- Syntax highlighting for 30+ languages
- LSP support with auto-completion and diagnostics

* Sat Dec 07 2024 mfw <espadon@outlook.com> - 0.4.0-1
- Initial RPM release of fackr
- Terminal text editor written in Rust
- VSCode-style keybindings
- Rope-based text buffer
