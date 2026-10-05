# Contributing

Thanks for your interest in improving `ecocash`. This crate is still incomplete: it has not been verified against the live EcoCash sandbox (see the warning in the [README](README.md)), so reports from anyone who can exercise the real API are especially welcome.

## Getting started

1. Fork the repository and clone your fork.
2. Create a branch from `main`: `git checkout -b feat/short-description`.
3. Make your change, then run the checks below.
4. Open a pull request against `main`.

You need a recent stable Rust toolchain. No credentials are required to build or run the test suite.

## Checks

Run all of these before pushing; they must pass:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Tests run against a [`wiremock`](https://crates.io/crates/wiremock) mock server, so they need no network. When you add or change behaviour, add or update a test in `tests/client.rs` that checks the request path, headers and body, or the parsing of the response.

## Code style

- Follow `rustfmt` defaults and keep clippy clean.
- Public items need a short doc comment.
- Keep response types lenient: fields are `Option` and unknown fields go in `extra`, because the API schema is loosely documented.
- Prefer small, focused changes. Don't mix refactors with behaviour changes.

## Commits

- Use [Conventional Commits](https://www.conventionalcommits.org/) style: `feat:`, `fix:`, `docs:`, `test:`, `chore:`, `refactor:`.
- Keep commits atomic: one logical change per commit, each one building and passing the tests.
- Explain the *why* in the commit body when it isn't obvious.

## Testing against the sandbox

The `sandbox` and `certify` examples call the real EcoCash sandbox. They read credentials from environment variables (see the README) and never from files.

**Never commit credentials, Basic auth headers, merchant PINs, or real customer numbers.** `.env` and `certification.csv` are git-ignored; keep it that way. If you accidentally expose a credential, regenerate it on the EcoCash developer portal.

When you report sandbox behaviour (status values, error bodies, field names), redact the `Authorization` header and any personal phone numbers.

## Reporting issues

Open an issue with:

- what you did (the call and the request fields, minus secrets),
- what you expected,
- what happened, including the HTTP status and response body,
- your Rust version (`rustc --version`) and the crate commit.

Differences between the real API's responses and what this crate assumes are the most useful reports right now.

## License

By contributing you agree that your contributions are licensed under the project's [MIT license](LICENSE).
