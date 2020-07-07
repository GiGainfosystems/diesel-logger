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

## Version 0.3.0
- Uses a generic trait implementations to make the logger universal over all diesel
connections.
- Update diesel dependency to use v2.0.0 (currently on https://github.com/diesel-rs/diesel)
