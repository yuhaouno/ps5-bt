# HCI diagnostic fixtures

`local-version.bin` is a hand-constructed Command Complete event:

```text
0e 0c 01 01 10 00 09 34 12 09 cd ab 78 56
```

All values are synthetic. No address, key, console capture, or upstream test
code is included. See docs/PROTOCOL.md for the specification/section baseline.
The decoder reports raw numeric values and does not map them to hardware models.
