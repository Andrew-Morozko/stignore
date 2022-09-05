# stignore

Quickly add [Syncthing](https://syncthing.net)'s [ignore patterns](https://docs.syncthing.net/users/ignoring) from the terminal.

Status: I consider this project to be "done". I'm really limited timewise, so expect only critical fixes (which are pretty unlikely) and no new features from me (PRs are always welcome).

## Installation

From source: `cargo install stignore --git https://github.com/Andrew-Morozko/stignore.git`

Download precompiled binaries from the [Releases](https://github.com/Andrew-Morozko/stignore/releases/latest) page.

If you want `stignore` to appear in your package manager of choice &ndash; feel free to create a PR.

## Examples
In all examples syncthing folder is located at `/path_to/syncthing_folder/` and current working directory is `/path_to/syncthing_folder/some/path/inside`

---

By default `stignore` modifies your patterns like this:

`stignore 'ba{r,z}/*.png' ./foo/ignore_me`
```
/some/path/inside/ba{r,z}/*.png
/some/path/inside/foo/ignore_me
```

So it does following unreadable thing: prepends the current working directory relative to syncthing folder's root to your patterns.

---

`stignore` is aware of (but does not validate) [`.stignore` syntax](https://docs.syncthing.net/users/ignoring#patterns):

`stignore '// Comment' '(?d)**/.git' '#include extra_patterns.txt'`
```
// Comment
(?d)/some/path/inside/**/.git
#include /some/path/inside/extra_patterns.txt
```

---

To disable path prepending use `--absolute` option. It copies provided patterns as-is:

`stignore --absolute '(?d)Thumbs.db' '(?d).DS_Store'`
```
(?d)Thumbs.db
(?d).DS_Store
```
---

If you want to make sure that `stignore` will do what you expect &ndash; use `--preview` flag. `stignore` will print planned changes and ask you to confirm them.

`stignore --absolute --preview (?d)Thumbs.db`
```
Appending to /path_to/syncthing_folder/.stignore:
(?d)Thumbs.db
Proceed? (Y/n) █
```

In case you want to reduce `stignore`'s chattiness &ndash; provide `--silent` flag.

---

### .stignore_sync

`.stignore` files are local to each machine, but I wanted my ignore patterns to be synchronized, so I created the following homebrew convention:

Shared ignore patterns are placed in `.stignore_sync` file (it is synced just like any other file), and I `#include` it in each local `.stignore`. This way pattern in `.stignore_sync` will be applied on all remote devices.

By default `stignore` looks for a `#include .stignore_sync` statement in `.stignore` file. If it's found &ndash; patterns are appended to `.stignore_sync`, otherwise &ndash; to `.stignore`.

You can override this behavior by supplying `--target stignore` or `--target stignore_sync`.

## Contributing

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as below, without any additional terms or conditions.

## License

&copy; 2022 Andrew Morozko.

This project is licensed under either of

- [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0) ([`LICENSE-APACHE`](LICENSE-APACHE))
- [MIT license](https://opensource.org/licenses/MIT) ([`LICENSE-MIT`](LICENSE-MIT))

at your option.