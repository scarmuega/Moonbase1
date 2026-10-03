#!/usr/bin/env python3
"""Name-only conversion of the founder-selected experimental source (stdlib only).

Usage: python3 tools/prepare_hab_model.py PATH_TO_SOURCE_GLB
Run from workspace root. Geometry/texture binary chunks are copied byte-for-byte.
"""
import hashlib
import json
from pathlib import Path
import struct
import sys

SOURCE_SHA256 = "553e4a8a6c9ed28336db5219c4f4ab6ecb7f3e9a32eaf5b743058f2e956857f0"


def convert(source):
    assert hashlib.sha256(source).hexdigest() == SOURCE_SHA256, "unexpected source hash"
    magic, version, length = struct.unpack_from("<III", source)
    assert magic == 0x46546C67 and version == 2 and length == len(source)
    json_length, kind = struct.unpack_from("<II", source, 12)
    assert kind == 0x4E4F534A
    document = json.loads(source[20:20 + json_length])
    root, = document["scenes"][document.get("scene", 0)]["nodes"]
    assert document["nodes"][root]["name"] == "hab-v2"
    document["nodes"][root]["name"] = "hab"
    encoded = json.dumps(document, separators=(",", ":")).encode()
    encoded += b" " * (-len(encoded) % 4)
    rest = source[20 + json_length:]
    return (struct.pack("<III", magic, version, 20 + len(encoded) + len(rest))
            + struct.pack("<II", len(encoded), kind) + encoded + rest)


if __name__ == "__main__":
    check = sys.argv[1] == "--check"
    source_path = Path(sys.argv[2] if check else sys.argv[1])
    delivered = convert(source_path.read_bytes())
    target = Path(__file__).resolve().parents[1] / "assets/models/hab.glb"
    if check:
        assert target.read_bytes() == delivered, "delivered asset is not the name-only conversion"
        json_length, = struct.unpack_from("<I", delivered, 12)
        document = json.loads(delivered[20:20 + json_length])
        for mesh in document["meshes"]:
            for primitive in mesh["primitives"]:
                mat = document["materials"][primitive["material"]]
                pbr = mat.get("pbrMetallicRoughness", {})
                for texture in (pbr.get("baseColorTexture"), pbr.get("metallicRoughnessTexture"), mat.get("normalTexture")):
                    if texture is not None:
                        coordinate = texture.get("texCoord", 0)
                        assert coordinate >= 0 and f"TEXCOORD_{coordinate}" in primitive["attributes"]
        assert all("bufferView" in image and "uri" not in image for image in document["images"])
        assert all("uri" not in buffer for buffer in document["buffers"])
        print("PASS name-only conversion, unchanged binary chunks, embedded images and UV bindings")
    else:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(delivered)
    print(f"source_sha256={SOURCE_SHA256}")
    print(f"delivered_sha256={hashlib.sha256(delivered).hexdigest()}")
