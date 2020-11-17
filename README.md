# io-backplane

A low-level framework for blazingly fast, portable, asynchronous I/O.

Supports:

* Linux io-uring, via [ringbahn](https://github.com/ringbahn/ringbahn/).
* Standard I/O, on a threadpool via [blocking](https://github.com/smol-rs/blocking/).

## Status: Prealpha

Uring pulls us in one direction and traditional I/O pulls us in the
other. I've come up with a new API that I think could do both, but
it's yet another new async I/O API and we already have enough of those.

## Development

If you're on alpine (or another musl-based distro), you'll need some
forks I haven't published yet to get uring support. Pester me to
figure out what I need to send back upstream.

## Copyright and License

Copyright (c) 2020 James Laver, io-backplane Contributors

This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at http://mozilla.org/MPL/2.0/.
