# Changelog

All notable changes to this project are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `mx3_api_core`: error body, money, file export report and API level, split out of mx3 (CORE-1).
- `mx3_api_lab`: lab endpoint types, BOM logic and both clients, split out of mx3 (LAB-1).
- `client` and `sqlx` features, off by default: with none, serde types only (CORE-2).

### Changed

- Clients send `User-Agent: mx3_api/<version>`, the library's version, not the application's (CORE-2).
