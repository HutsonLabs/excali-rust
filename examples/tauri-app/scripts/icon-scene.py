#!/usr/bin/env python3
"""Writes src-tauri/icons/icon.excalidraw, the scene the app icon is drawn
from (scripts/icons.sh renders it with `excali render`): a rounded violet
tile holding a hand-drawn ellipse and a diamond. The element template is
the rectangle of the chrome-export fixtures (upstream's element shape)."""
import copy
import json
import pathlib

APP = pathlib.Path(__file__).resolve().parent.parent
ROOT = APP.parent.parent
TEMPLATE = ROOT / "crates/excali-cli/tests/fixtures/chrome-export/scenes/element-rectangle-with-link.excalidraw"


def element(template, kind, seed, x, y, w, h, **style):
    e = copy.deepcopy(template)
    e.update(id=f"icon-{kind}-{seed}", type=kind, seed=seed, versionNonce=seed,
             x=x, y=y, width=w, height=h, link=None, **style)
    return e


def main():
    scene = json.loads(TEMPLATE.read_text())
    t = scene["elements"][0]
    scene["elements"] = [
        # 1000 x 1000 with padding 12 exports as 1024 x 1024
        element(t, "rectangle", 1, 0, 0, 1000, 1000, strokeColor="#1e1e1e",
                backgroundColor="#6965db", fillStyle="solid", strokeWidth=4,
                roughness=1, roundness={"type": 3}),
        element(t, "ellipse", 2, 170, 190, 460, 380, strokeColor="#1e1e1e",
                backgroundColor="#ffec99", fillStyle="hachure", strokeWidth=4,
                roughness=2, roundness=None),
        element(t, "diamond", 3, 440, 420, 400, 400, strokeColor="#1e1e1e",
                backgroundColor="#ffffff", fillStyle="cross-hatch", strokeWidth=4,
                roughness=2, roundness={"type": 2}),
    ]
    scene["appState"]["viewBackgroundColor"] = "#ffffff"
    out = APP / "src-tauri/icons/icon.excalidraw"
    out.write_text(json.dumps(scene, indent=2) + "\n")
    print(out)


if __name__ == "__main__":
    main()
