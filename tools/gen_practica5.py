#!/usr/bin/env python3
"""Genera app/assets/practica5.json a partir del netlist de la Práctica 5.

El layout es propio: placa 1 = AND + NOT, placa 2 = OR, placa 3 = entradas,
display y LEDs. Las redes lógicas van con etiquetas; la alimentación, los
interruptores, el display y los LEDs van con cables.
"""
import csv
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "assets"

# (placa, columna del pin 1)
PLACE = {
    "U1": (1, 1), "U2": (1, 8), "U3": (1, 15), "U4": (1, 22),
    "U5": (1, 29), "U6": (1, 36), "U14": (1, 43),
    "U9": (2, 1), "U10": (2, 8), "U11": (2, 15), "U12": (2, 22),
    "U13": (2, 29),
}
MODEL = {f"U{i}": "74HC08" for i in range(1, 7)}
MODEL.update({f"U{i}": "74HC32" for i in range(9, 14)})
MODEL["U14"] = "74HC04"

QUAD = [([1, 2], 3), ([4, 5], 6), ([9, 10], 8), ([12, 13], 11)]
HEX = [([1], 2), ([3], 4), ([5], 6), ([9], 8), ([11], 10), ([13], 12)]
GATES = {"74HC08": QUAD, "74HC32": QUAD, "74HC04": HEX}

FREE_INPUTS = {
    "U6": [4, 5, 9, 10, 12, 13],
    "U13": [12, 13],
    "U14": [5, 9, 11, 13],
}


def hole(board, row, col):
    """Nombre de un orificio principal (row 0..9 = a..j)."""
    name = f"{chr(ord('a') + row)}{col}"
    return name if board == 1 else f"{name}@{board}"


def pin_hole(chip, pin):
    board, col = PLACE[chip]
    if pin <= 7:
        return board, 5, col + pin - 1
    return board, 4, col + 14 - pin


def label_hole(h):
    """Etiqueta en la fila adyacente, dentro del mismo grupo de 5 orificios."""
    board, row, col = h
    return board, (6 if row == 5 else 3), col


def parse_pins(text):
    return [(m.group(1), int(m.group(2)))
            for m in re.finditer(r"(U\d+)[A-F]?\s+pin\s+(\d+)", text)]


def load_netlist():
    rows = list(csv.DictReader(open(ASSETS / "red_12ci.csv", encoding="utf-8")))
    nets = {}
    for r in rows:
        name = r["red"].strip()
        driver = parse_pins(r["driver"])
        consumers = parse_pins(r["consumidores"])
        nets[name] = {"driver": driver[0] if driver else None, "consumers": consumers}
    return nets


def check_netlist(nets):
    pin_net = {}
    for name, n in nets.items():
        for chip, pin in ([n["driver"]] if n["driver"] else []) + n["consumers"]:
            if chip not in PLACE:
                raise SystemExit(f"red {name}: chip desconocido {chip}")
            if not 1 <= pin <= 14:
                raise SystemExit(f"red {name}: pin fuera de rango {chip} {pin}")
            if (chip, pin) in pin_net:
                raise SystemExit(
                    f"el pin {chip} {pin} aparece en dos redes: "
                    f"{pin_net[(chip, pin)]} y {name}")
            pin_net[(chip, pin)] = name
    for chip, model in MODEL.items():
        for ins, out in GATES[model]:
            for p in ins:
                if (chip, p) not in pin_net and p not in FREE_INPUTS.get(chip, []):
                    raise SystemExit(f"falta el pin {chip} {p} en el netlist")
            # Una compuerta usada tiene su salida en el netlist; una libre no.
            if any((chip, p) in pin_net for p in ins) and (chip, out) not in pin_net:
                raise SystemExit(f"falta la salida {chip} {out} en el netlist")
    for name, n in nets.items():
        if n["driver"]:
            chip, pin = n["driver"]
            outs = [o for _, o in GATES[MODEL[chip]]]
            if pin not in outs:
                raise SystemExit(f"red {name}: {chip} pin {pin} no es salida")
    return pin_net


def eval_netlist(nets, pin_net, bits):
    val = {name: 0 for name in nets}
    for name, bit in zip("ABCDE", bits):
        val[name] = bit
    for _ in range(20):
        changed = False
        for chip, model in MODEL.items():
            for ins, out in GATES[model]:
                if (chip, out) not in pin_net:
                    continue  # compuerta libre: salida sin conectar
                name = pin_net[(chip, out)]
                xs = [val[pin_net[(chip, p)]] for p in ins]
                if model == "74HC04":
                    v = 0 if xs[0] else 1
                elif model == "74HC08":
                    v = int(all(xs))
                else:
                    v = int(any(xs))
                if val[name] != v:
                    val[name] = v
                    changed = True
        if not changed:
            return val
    raise SystemExit("el netlist no converge (¿hay un lazo?)")


def display_char(s, s2):
    # a,b,c,d,e,f,g
    seg = [s2, s, s, s2, s2, s2, s2]
    table = {
        (1, 1, 1, 1, 1, 1, 0): "0",
        (0, 1, 1, 0, 0, 0, 0): "1",
        (1, 1, 0, 1, 1, 0, 1): "2",
        (1, 1, 1, 1, 0, 0, 1): "3",
        (0, 1, 1, 0, 0, 1, 1): "4",
        (1, 0, 1, 1, 0, 1, 1): "5",
        (1, 0, 1, 1, 1, 1, 1): "6",
        (1, 1, 1, 0, 0, 0, 0): "7",
        (1, 1, 1, 1, 1, 1, 1): "8",
        (1, 1, 1, 1, 0, 1, 1): "9",
        (1, 0, 0, 1, 1, 1, 1): "E",
    }
    return table.get(tuple(seg), "?")


def verify_checklist(nets, pin_net):
    rows = list(csv.DictReader(open(ASSETS / "checklist_32.csv", encoding="utf-8")))
    if len(rows) != 32:
        raise SystemExit(f"el checklist debe tener 32 filas, tiene {len(rows)}")
    ok = 0
    for r in rows:
        bits = [int(r[k]) for k in "ABCDE"]
        val = eval_netlist(nets, pin_net, bits)
        f = val["F"]
        x = val["T"]
        ch = display_char(val["S"], val["S2"])
        if (f, x, ch) != (int(r["F"]), int(r["X"]), r["display"]):
            raise SystemExit(
                f"fila {r['n']} no coincide: F={f} X={x} display={ch}; "
                f"esperado F={r['F']} X={r['X']} display={r['display']}")
        ok += 1
    print(f"{ok}/32")


class Builder:
    def __init__(self):
        self.comps = []
        self.used = {}
        self.n = {}

    def new_id(self, prefix):
        k = self.n.get(prefix, 0) + 1
        while any(c["id"] == f"{prefix}{k}" for c in self.comps):
            k += 1
        self.n[prefix] = k
        return f"{prefix}{k}"

    def add(self, cid, tipo, pines, **extra):
        key = []
        for p in pines:
            m = re.fullmatch(r"([a-j])(\d+)(?:@(\d+))?", p)
            r = re.fullmatch(r"([tb])([+-])(\d+)(?:@(\d+))?", p)
            if m:
                key.append((int(m.group(3) or 1), "m", m.group(1), int(m.group(2))))
            elif r:
                key.append((int(r.group(4) or 1), "r", r.group(1) + r.group(2), int(r.group(3))))
            else:
                raise SystemExit(f"{cid}: orificio inválido {p}")
        for k in key:
            if k in self.used:
                raise SystemExit(f"colisión en {p}: {self.used[k]} y {cid}")
            self.used[k] = cid
        c = {"id": cid, "tipo": tipo, "pines": pines}
        c.update({k: v for k, v in extra.items() if v is not None})
        self.comps.append(c)
        return cid

    def label(self, board, row, col, name):
        self.add(self.new_id("L"), "etiqueta", [hole(board, row, col)], nombre=name)


def build(nets, pin_net):
    b = Builder()
    b.add("F1", "fuente", ["t+1", "t-1"], valor=5.0)
    for chip in sorted(PLACE, key=lambda x: int(x[1:])):
        board, col = PLACE[chip]
        b.add(chip, "chip", [hole(board, 5, col)], modelo=MODEL[chip])

    # Placa 3: interruptores, pull-downs y etiquetas A-E.
    switch_cols = [1, 4, 7, 10, 13]
    for i, col in enumerate(switch_cols):
        b.add(f"S{i + 1}", "interruptor",
              [hole(3, 7, col), hole(3, 7, col + 1)])
        b.add(f"R{i + 1}", "resistencia",
              [hole(3, 8, col + 1), f"b-{col + 1}@3"], valor=10000.0)
        b.add(b.new_id("W"), "cable",
              [hole(3, 9, col), f"t+{col + 2}@3"], color="rojo")

    # Etiquetas de las entradas en la placa 3.
    for i, col in enumerate(switch_cols):
        b.label(3, 9, col + 1, "ABCDE"[i])

    # Display: 7 resistencias de 470 ohmios y etiquetas S/S2.
    for i in range(7):
        c = 20 + 2 * i
        b.add(f"R{6 + i}", "resistencia",
              [hole(3, 8, c), hole(3, 8, c + 1)], valor=470.0)
        net = "S2" if i in (0, 3, 4, 5, 6) else "S"
        b.label(3, 9, c + 1, net)
    b.add("DS1", "display",
          [hole(3, 7, 20 + 2 * i) for i in range(7)] + [hole(3, 7, 34)])
    b.add(b.new_id("W"), "cable", [hole(3, 8, 34), "b-34@3"], color="negro")

    # LEDs de faltan (F) y sobran (T).
    b.add("R13", "resistencia", [hole(3, 8, 38), hole(3, 8, 39)], valor=470.0)
    b.label(3, 9, 38, "F")
    b.add("D1", "led", [hole(3, 7, 39), hole(3, 7, 41)], color="rojo")
    b.add(b.new_id("W"), "cable", [hole(3, 8, 41), "b-41@3"], color="negro")
    b.add("R14", "resistencia", [hole(3, 8, 44), hole(3, 8, 45)], valor=470.0)
    b.label(3, 9, 44, "T")
    b.add("D2", "led", [hole(3, 7, 45), hole(3, 7, 47)], color="verde")
    b.add(b.new_id("W"), "cable", [hole(3, 8, 47), "b-47@3"], color="negro")

    # Alimentación: fuente, enlaces de rieles y Vcc/GND de cada chip.
    for board in (1, 2, 3):
        b.add(b.new_id("W"), "cable", [f"t+60@{board}", f"b+60@{board}"], color="rojo")
        b.add(b.new_id("W"), "cable", [f"t-60@{board}", f"b-60@{board}"], color="negro")
    for board in (2, 3):
        b.add(b.new_id("W"), "cable", [f"t+{56 + board}@1", f"t+1@{board}"], color="rojo")
        b.add(b.new_id("W"), "cable", [f"t-{56 + board}@1", f"t-1@{board}"], color="negro")
    for chip in PLACE:
        board, col = PLACE[chip]
        b.add(b.new_id("W"), "cable",
              [hole(board, 3, col), f"t+{col + 8}@{board}"], color="rojo")
        b.add(b.new_id("W"), "cable",
              [hole(board, 6, col + 6), f"b-{col + 8}@{board}"], color="negro")

    # Etiquetas de todas las redes del netlist.
    placed = set()
    for name, n in nets.items():
        pts = []
        if n["driver"]:
            pts.append(n["driver"])
        pts.extend(n["consumers"])
        for chip, pin in pts:
            board, row, col = label_hole(pin_hole(chip, pin))
            if (board, row, col) not in placed:
                placed.add((board, row, col))
                b.label(board, row, col, name)

    # Entradas de compuertas libres a GND y una etiqueta GND en el riel negativo.
    for chip, pins in FREE_INPUTS.items():
        for pin in pins:
            board, row, col = label_hole(pin_hole(chip, pin))
            if (board, row, col) in placed:
                raise SystemExit(f"GND choca con otra etiqueta en {hole(board, row, col)}")
            placed.add((board, row, col))
            b.label(board, row, col, "GND")
    b.add(b.new_id("L"), "etiqueta", ["b-1"], nombre="GND")

    return {
        "nombre": "Práctica 5 — detector 2 de 5 (12 CI)",
        "columnas": 60,
        "tableros": 3,
        "componentes": b.comps,
    }


def main():
    nets = load_netlist()
    pin_net = check_netlist(nets)
    verify_checklist(nets, pin_net)
    circ = build(nets, pin_net)
    out = ASSETS / "practica5.json"
    out.write_text(json.dumps(circ, ensure_ascii=False, indent=2) + "\n",
                   encoding="utf-8")
    print(f"escrito {out} con {len(circ['componentes'])} componentes")


if __name__ == "__main__":
    sys.exit(main())
