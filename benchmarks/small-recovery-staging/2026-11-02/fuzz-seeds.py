"""Generate the targeted 72-case Leopard fuzz smoke-test seed corpus."""
from pathlib import Path
import sys

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
index = 0
for family in (1, 2):
    for data in (4, 8):
        for parity in (2, 4):
            for length_index in (0, 1, 2):
                for api in (0, 1, 2):
                    header = bytearray(16)
                    header[0] = family
                    header[3] = data - 1
                    header[4] = parity - 1
                    header[5] = length_index
                    header[7] = api
                    header[8] = min(2, parity)
                    header[10:12] = b"\xff\xff"
                    header[12] = 1
                    header[14] = data - 1
                    header[15] = 3
                    (out / f"seed-{index:03}").write_bytes(header + b"\x11\x27\x5a\xc3")
                    index += 1
