# Frozen configuration fixtures

These files record released wire contracts. Keep their bytes unchanged when
updating the current format; migration tests must read the historical shape.

The v4 default EAV and JSON were exported by commit
`91805422ae1ba3ee4b2f34dccd027cd0cd039b59` before orientation fields were added.

| File | SHA-256 |
| --- | --- |
| v4-default.eav | bda31315cbab2c63d4672c925126dfb5d1262d64e97fac95a1defd13617480f1 |
| v4-default.json | 914c0c3c91a474b4fdeba65b5098db8eab76360a8478837f69d107a4575b87d3 |

The v5 default fixtures were copied byte-for-byte from configs at
`168cb0d7a5bd54513ad1ffcfc41ecd8c855e3e03` before adding BLE.

| File | SHA-256 |
| --- | --- |
| v5-default.eav | d76812cb684a74c59d96890ac5edd3d73ddaf8ba6945e8ba84fc268d3de5242b |
| v5-default.json | a712c0f47c96fb3042c339e5b3bdf285b59023b9d009822e9703c67598cb5d9e |
