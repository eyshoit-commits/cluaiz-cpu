# 1BitShit rebranding boundary

The public product is **1BitShit CPU** and the primary executable is `bitshit`.

The following legacy names intentionally remain during the compatibility phase:

- Rust packages and imports such as `cluaiz-shared` and `cluaiz_api`
- exported C ABI symbols beginning with `cluaiz_`
- existing persisted installation directories
- CEL and plugin identifiers already consumed by manifests

These names are implementation compatibility contracts, not the public brand. They must not be globally replaced until the dynamic-driver ABI, registry manifests and on-disk migration have explicit versions and compatibility tests.

New public documentation, release assets and user-facing CLI text should use `1BitShit CPU` or `bitshit`.
