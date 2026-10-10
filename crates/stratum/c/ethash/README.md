ProgPoW 0.9.4 as Ravencoin configures it (KawPoW), for checking shares.

`crypto/ethash/` is copied unchanged from `src/crypto/ethash/` of
https://github.com/RavenProject/Ravencoin at commit
6d48ae0175b10283248146ae3080e2ba70966739, without `managed.cpp` (global contexts,
not used here). It is the "ethash" library by Paweł Bylica, licensed under the
Apache License, Version 2.0: https://www.apache.org/licenses/LICENSE-2.0

`../kawpow_shim.cpp` is this project's own C interface to it.
