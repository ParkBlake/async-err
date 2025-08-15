# Changelog
All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- Initial implementation of `async-err` providing contextual asynchronous error handling in Rust.
- Async extension traits `.with_context()` and `.and_then_async()` for adding error context and chaining futures.
- `AsyncError` wrapper type that enriches underlying errors with detailed context.
- Hook system for global async error capture, logging, and processing.
- Optional timestamp support in hooks output, enabled via the `chrono` feature.

### Fixed
- None yet.

### Changed
- None yet.

---

## [0.1.0] - 15 August 2025

- First stable release including core features for async error context and hooks.

---

## Future

Planned enhancements include:
- More customisable hook implementations and additional hook event types.
- Integration support with popular async runtime error utilities.
- Ergonomics improvements for handling complex async error flows.
- Additional timestamp format options and localisation support.
- `AsyncError` support for multiple context layers and structured context data.
- Additional async combinators for common error handling patterns.
