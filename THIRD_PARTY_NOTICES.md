# Third-party notices

LOTUS itself is licensed under the GNU General Public License v3.0 or later (see `LICENSE`). It bundles or links the following components, each under its own license.

## Bundled programs and data

| Component | Version | License | Source |
|---|---|---|---|
| sing-box (`sing-box.exe`) | 1.14.4 | GPL-3.0-or-later | https://github.com/SagerNet/sing-box |
| Cronet / libcronet (`libcronet.dll`, NaiveProxy support, shipped in the sing-box release) | as shipped with sing-box 1.14.4 | BSD-3-Clause (Chromium) | https://github.com/SagerNet/cronet-go, https://chromium.googlesource.com/chromium/src |
| wintun (embedded in sing-box) | as shipped with sing-box 1.14.4 | GPL-2.0 / prebuilt redistributable license | https://www.wintun.net |
| sing-geosite `category-ads-all` rule-set (`rules/block-trackers.srs`) | commit 23066e3b | GPL-3.0 (data from v2fly/domain-list-community, MIT) | https://github.com/SagerNet/sing-geosite |

sing-box runs as a separate program that LOTUS starts and talks to over a local interface. It is distributed unmodified, exactly as published by its authors (see `vendor/LOCK` for the verified SHA-256 digests). The corresponding source code is available at the URL above (tag `v1.14.4`). The full GPL-3.0 text is at https://www.gnu.org/licenses/gpl-3.0.txt and in `licenses/SING-BOX-LICENSE.txt`.

## Libraries compiled into LOTUS

- Tauri 2 and plugins (single-instance, autostart, dialog, clipboard-manager): MIT or Apache-2.0
- React and React DOM: MIT
- Radix UI primitives, shadcn/ui components, Tailwind CSS, tw-animate-css, class-variance-authority, cn: MIT
- Motion, Sonner, NumberFlow: MIT
- Lucide icons: ISC
- Inter typeface (https://github.com/rsms/inter, via @fontsource-variable/inter): SIL Open Font License 1.1
- tokio, serde, serde_json, reqwest, rustls, ring, windows-sys, windows-service, url, uuid, base64, sha2, tracing and their dependencies: MIT and/or Apache-2.0 (ring: ISC-style and OpenSSL-derived terms)

Generate an exhaustive, per-crate list with:

```
cargo install cargo-about
cargo about generate about.hbs > licenses.html
```
