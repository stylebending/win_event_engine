<p align="center"><img src="https://readme-typing-svg.herokuapp.com?font=Fira+Code&pause=1000&center=true&width=435&lines=WinEventEngine;Simple+Configuration;Powerful+Results" alt="WinEventEngine" /></p>

<p align="center">

<a href="https://github.com/stylebending/win_event_engine/releases">
  <img src="https://img.shields.io/github/v/release/stylebending/win_event_engine?style=for-the-badge&color=darkgreen&logo=git&logoColor=white&label=Release&labelColor=darkgreen">
</a>

<a href="https://github.com/stylebending/win_event_engine/releases">
  <img src="https://img.shields.io/github/actions/workflow/status/stylebending/win_event_engine/.github/workflows/release.yml?style=for-the-badge&color=darkgreen&logo=github&logoColor=white&label=Build&labelColor=darkgreen">
</a>

<a href="https://github.com/stylebending/win_event_engine/releases">
  <img src="https://img.shields.io/github/downloads/stylebending/win_event_engine/total?color=darkgreen&logo=github&label=Total%20Downloads&style=for-the-badge&labelColor=darkgreen">
</a>

</p>

<br>

<h3 align="center">Quick Navigation</h3>

<p align="center">

<a href="https://github.com/stylebending/win_event_engine/wiki">
  <img src="https://img.shields.io/badge/📖-Documentation%20Wiki-darkblue?style=for-the-badge&labelColor=darkblue">
</a>

<a href="https://github.com/stylebending/win_event_engine/wiki#first-time-setup-5-minutes">
  <img src="https://img.shields.io/badge/🚀-First%20Time%20Setup-darkblue?style=for-the-badge&labelColor=darkblue">
</a>

<a href="https://github.com/stylebending/win_event_engine/wiki#next-steps">
  <img src="https://img.shields.io/badge/🧭-Next%20Steps-darkblue?style=for-the-badge&labelColor=darkblue">
</a>

</p>

<p align="center">

<a href="https://github.com/stylebending/win_event_engine/wiki/Configuration-Reference">
  <img src="https://img.shields.io/badge/⚙️-Configuration-darkblue?style=for-the-badge&labelColor=darkblue">
</a>

<a href="https://github.com/stylebending/win_event_engine/wiki/Lua-Scripting-API">
  <img src="https://img.shields.io/badge/🔧-Lua%20Scripting-darkblue?style=for-the-badge&labelColor=darkblue">
</a>

<a href="https://github.com/stylebending/win_event_engine/wiki#common-commands">
  <img src="https://img.shields.io/badge/💻-Commands-darkblue?style=for-the-badge&labelColor=darkblue">
</a>

</p>

<p align="center">

<a href="https://github.com/stylebending/win_event_engine/wiki#running-as-a-windows-service">
  <img src="https://img.shields.io/badge/🪟-Windows%20Service-darkblue?style=for-the-badge&labelColor=darkblue">
</a>

<a href="https://github.com/stylebending/win_event_engine/wiki/Web-Dashboard">
  <img src="https://img.shields.io/badge/📊-Web%20Dashboard-darkblue?style=for-the-badge&labelColor=darkblue">
</a>

</p>

<p align="center">

<a href="https://github.com/stylebending/win_event_engine/blob/main/LICENSE">
  <img src="https://img.shields.io/badge/📄-MIT%20License-darkblue?style=for-the-badge&labelColor=darkblue">
</a>

<a href="https://github.com/stylebending/win_event_engine/blob/main/CONTRIBUTING.md">
  <img src="https://img.shields.io/badge/🪽-Contributing-darkblue?style=for-the-badge&labelColor=darkblue">
</a>

</p>

<br>

## 📦 What is Win Event Engine?

Win Event Engine is an event-driven automation framework for Windows:
- Play/pause media when focusing specific windows
- Auto commit changes to your config/dot files/folders
- Auto build/test an application under configurable conditions
- Get Discord/Slack/Telegram notifications for configurable events
- Write easy-to-learn Lua scripts to customize everything
- Watch it all happen in real-time on the web dashboard
- Much more! Simple configuration, powerful results

## Key Features

- **Event Monitoring**: File system, windows, processes, and registry
- **Rule Engine**: Pattern-based matching with Lua scripting
- **Web Dashboard**: Real-time monitoring at `http://localhost:9090`
- **Windows Service**: Run as background service
- **Plugin System**: Write custom actions in Lua

## Support

- 📖 [Documentation Wiki](https://github.com/stylebending/win_event_engine/wiki)
- 💡 [Discussions](https://github.com/stylebending/win_event_engine/discussions) for feature requests and questions
- 🐛 [Issue Tracker](https://github.com/stylebending/win_event_engine/issues) for bug reports and issues

## For Developers

**Build from Source** (requires [Rust](https://rustup.rs/)):

```bash
git clone https://github.com/stylebending/win_event_engine.git
cd win_event_engine
cargo build --release -p engine
# Executable: target/release/engine.exe
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
