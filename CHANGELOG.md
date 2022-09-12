# Changelog

## Unreleased

## 0.4
 - Bump to 2021 edition.
 - The function `Lockfile::create` has been renamed to `Lockfile::create_with_parents`.
 - A new function `Lockfile::create` has been added that creates a lockfile but fails
   if parent directories do not exist.
 - Make logging optional (add the 'log' feature to get previous behaviour). Crate now
   has no dependencies by default.
 - `Error` now implements `std::error::Error`.

## 0.2.2
 - Removed all unsafe and `#[forbid(unsafe)]`. No change in functionality

## 0.2.1
 - Slightly improved logging methods

## 0.2
 - Rename `close` to `release` to better match semantics.

## 0.1
 - Initial release
