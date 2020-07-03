## Version 0.1.1
- Resurrect source code repository
- Add maintenance status to Cargo.toml

## Version 0.2.0
- Updated dependencies to be compatible with GST.
- Updated Cargo.toml.
    - Updated diesel to 1.4.3.
    - Added `chrono` for date/time.
- Reported the R2D2 adapted version of the main library.
- Split main crate into `lib`, `postgres`, and `oci`.
- Customised logging levels to use environmental variable.

## Version 0.2.1
- Made the implementations for `R2D2` more correct from a theoretical programming
point of view.
