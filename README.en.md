# StudyTracker Desktop

[繁體中文](README.md)

StudyTracker Desktop is a personal desktop application for study planning, focus timing, and activity review. It follows local-first and privacy-first principles, with the goal of helping users manage their studies and retain ownership of their records without requiring an account or network connection.

> A runnable engineering scaffold and local data foundation are in place. Product features such as subjects, sessions, and activity tracking are still under development, and no production release is available yet.

## Core Direction

- Manage subjects, tasks, and daily study plans.
- Provide stopwatch, countdown, and Pomodoro timers.
- Record desktop application activity while allowing users to control what is recorded.
- Support focus through configurable blacklist, whitelist, and warning mechanisms.
- Provide local statistics, data export, backup, and restore.
- Keep core features available offline without requiring an account or cloud service.

All product features above are planned and should not be considered implemented.

## Current Foundation

- A Tauri 2, Rust, React, TypeScript, and Vite application scaffold.
- SQLite migrations, repository abstractions, and transaction boundaries.
- Foundations for IDs, clocks, time zones, settings, privacy-safe logging, and background coordination.
- An accessible application shell with light and dark themes and keyboard support.
- A unified entry point for frontend and Rust formatting, linting, type checks, tests, and builds.

## Development Requirements

- Node.js 24 or later
- npm 11 or later
- Rust 1.85 or later
- The system dependencies required by Tauri when building on Windows

## Getting Started

```powershell
npm install
npm run tauri:dev
```

Run the complete quality checks:

```powershell
npm run check
```

Build the desktop application:

```powershell
npm run tauri:build
```

The current build does not produce an installer. Production packaging and release work will be completed later.

## Documentation

- [Current architecture and development boundaries](docs/architecture.md)

## StudyTracker Project

Desktop is the desktop client for StudyTracker. The main project repository has not been created yet; its official link will be added when available.

Additional desktop and mobile versions are planned for future development according to the capabilities of each operating system and device.

## License

This project is licensed under the [Apache License 2.0](LICENSE).
