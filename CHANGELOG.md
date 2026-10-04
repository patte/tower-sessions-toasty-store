# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/patte/tower-sessions-toasty-store/compare/v0.1.1...v0.2.0) - 2026-10-04

Breaking: requires Toasty 0.11

### Changed

- bump `toasty` from 0.8.0 to 0.11.0
- `create` and `save` are now a single atomic upsert statement on PostgreSQL,
  SQLite and Turso; MySQL keeps the check-then-write fallback
- retry write conflicts on a 1ms→32ms backoff for up to 5s, instead of 8 tries
  up to 160ms; under heavy contention an operation can now take up to 5s
  before it fails
- README: limitations table updated for Toasty 0.11

## [0.1.1](https://github.com/patte/tower-sessions-toasty-store/compare/v0.1.0...v0.1.1) - 2026-07-18

### Other

- add codecov badge to README

## [0.1.0]

### Added

- Initial release: `ToastyStore` implementing `SessionStore` and
  `ExpiredDeletion` on top of Toasty 0.8, tested against SQLite, Turso,
  PostgreSQL, and MySQL.
