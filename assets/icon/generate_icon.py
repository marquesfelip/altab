from PIL import Image, ImageDraw, ImageFilter
import sys

OUT = sys.argv[1]
S = 1024  # render grande e reduz (supersampling)

def lerp(a, b, t): return int(a + (b - a) * t)

def gradient(size):
    # diagonal #0A84FF -> #5E5CE6
    c0, c1 = (10, 132, 255), (94, 92, 230)
    g = Image.new("RGB", (size, size))
    px = g.load()
    for y in range(size):
        for x in range(size):
            t = (x + y) / (2 * (size - 1))
            px[x, y] = tuple(lerp(c0[i], c1[i], t) for i in range(3))
    return g

def rrect_mask(size, box, r):
    m = Image.new("L", (size, size), 0)
    ImageDraw.Draw(m).rounded_rectangle(box, r, fill=255)
    return m

grad = gradient(S)
base = Image.new("RGBA", (S, S), (0, 0, 0, 0))
# "squircle" do ícone (margem de 6% como nos ícones do macOS)
m = int(S * 0.06)
base.paste(grad, mask=rrect_mask(S, (m, m, S - m, S - m), int(S * 0.225)))

def box(l, t, r, b): return tuple(int(v * S) for v in (l, t, r, b))

back = box(0.18, 0.21, 0.64, 0.63)
front = box(0.36, 0.37, 0.82, 0.79)
rad = int(S * 0.055)
gap = int(S * 0.035)

d = ImageDraw.Draw(base)
# janela de trás (levemente translúcida)
back_layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ImageDraw.Draw(back_layer).rounded_rectangle(back, rad, fill=(255, 255, 255, 205))
base = Image.alpha_composite(base, back_layer)
# recorte (espaço) ao redor da janela da frente, preenchido com o gradiente
exp = (front[0] - gap, front[1] - gap, front[2] + gap, front[3] + gap)
cut = rrect_mask(S, exp, rad + gap)
squircle = rrect_mask(S, (m, m, S - m, S - m), int(S * 0.225))
from PIL import ImageChops
cut = ImageChops.multiply(cut, squircle)
base.paste(grad, mask=cut)
# janela da frente
fl = Image.new("RGBA", (S, S), (0, 0, 0, 0))
fd = ImageDraw.Draw(fl)
fd.rounded_rectangle(front, rad, fill=(255, 255, 255, 255))
# barra de título com três pontos (some em tamanhos pequenos, o que é desejável)
bar_h = int(S * 0.085)
dots_y = front[1] + bar_h // 2 + int(S * 0.012)
for i, col in enumerate(((255, 95, 87), (254, 188, 46), (40, 200, 64))):
    cx = front[0] + int(S * 0.06) + i * int(S * 0.055)
    r = int(S * 0.016)
    fd.ellipse((cx - r, dots_y - r, cx + r, dots_y + r), fill=col + (255,))
base = Image.alpha_composite(base, fl)

# brilho suave no topo
shine = Image.new("RGBA", (S, S), (0, 0, 0, 0))
sd = ImageDraw.Draw(shine)
sd.ellipse((-S * 0.2, -S * 0.75, S * 1.2, S * 0.45), fill=(255, 255, 255, 38))
shine = shine.filter(ImageFilter.GaussianBlur(S * 0.02))
shine = Image.composite(shine, Image.new("RGBA", (S, S), (0, 0, 0, 0)), squircle)
base = Image.alpha_composite(base, shine)

png = base.resize((256, 256), Image.LANCZOS)
png.save(f"{OUT}/altab.png")
sizes = [(16, 16), (20, 20), (24, 24), (32, 32), (40, 40), (48, 48), (64, 64), (128, 128), (256, 256)]
png.save(f"{OUT}/altab.ico", sizes=sizes)
base.resize((512, 512), Image.LANCZOS).save(f"{OUT}/altab-512.png")
print("ok")
