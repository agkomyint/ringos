gate = 0
# Shooter: the 0.3.0 showcase (for, const, comments, overlap, shake, palette)
const NB 8
const NE 6
const NS 24
const NP 24
bxs = new NB
bys = new NB
bon = new NB
exs = new NE
eys = new NE
eon = new NE
evy = new NE
sx = new NS
sy = new NS
pxs = new NP
pys = new NP
pvx = new NP
pvy = new NP
plife = new NP
for i 0 NS
  sx[i] = rnd(320)
  sy[i] = rnd(200)
px = 150
score = 0
lives = 3
wave = 1
cool = 0
hurt = 0
flash = 0
best = load(0)
heart 1 == 1
  clear(0)
  for i 0 NS
    sy[i] = sy[i] + 1 + i % 3
    if sy[i] > 199
      sy[i] = 0
      sx[i] = rnd(320)
    pixel(sx[i], sy[i], 5 + i % 3 * 2)
  if lives > 0
    if key(0)
      px = px - 4
    if key(1)
      px = px + 4
    px = max(0, min(306, px))
    if cool > 0
      cool = cool - 1
    if key(4) and cool == 0
      for i 0 NB
        if bon[i] == 0 and cool == 0
          bon[i] = 1
          bxs[i] = px + 6
          bys[i] = 170
          cool = 8
          beep(900, 20)
    for i 0 NB
      if bon[i] == 1
        bys[i] = bys[i] - 6
        if bys[i] < 0
          bon[i] = 0
    for i 0 NE
      if eon[i] == 0
        if rnd(40) == 0
          eon[i] = 1
          exs[i] = rnd(300) + 4
          eys[i] = 0 - 10
          evy[i] = 1 + rnd(1 + wave)
      else
        eys[i] = eys[i] + evy[i]
        if eys[i] > 200
          eon[i] = 0
        for j 0 NB
          if bon[j] == 1 and eon[i] == 1 and overlap(bxs[j], bys[j], 2, 6, exs[i], eys[i], 14, 10)
            bon[j] = 0
            eon[i] = 0
            score = score + 10
            if score > best
              best = score
              save(0, best)
            beep(300, 60)
            shake(2)
            k = 0
            for p 0 NP
              if plife[p] == 0 and k < 8
                plife[p] = 12 + rnd(8)
                pxs[p] = exs[i] + 7
                pys[p] = eys[i] + 5
                pvx[p] = rnd(9) - 4
                pvy[p] = rnd(9) - 4
                k = k + 1
            if score / 200 + 1 > wave
              wave = wave + 1
              beep(1200, 150)
        if eon[i] == 1 and hurt == 0 and overlap(px, 172, 14, 10, exs[i], eys[i], 14, 10)
          eon[i] = 0
          lives = lives - 1
          hurt = 40
          flash = 6
          shake(8)
          beep(120, 300)
    if hurt > 0
      hurt = hurt - 1
  for p 0 NP
    if plife[p] > 0
      plife[p] = plife[p] - 1
      pxs[p] = pxs[p] + pvx[p]
      pys[p] = pys[p] + pvy[p]
      rect(pxs[p], pys[p], 2, 2, 12 - plife[p] / 3)
  if flash > 0
    palette(0, flash * 40, 0, 0)
    flash = flash - 1
    if flash == 0
      palette(0, 0, 0, 0)
  for i 0 NE
    if eon[i] == 1
      sprite(exs[i], eys[i], "a.....aaaaaaaaaaeaeaa.aaaaa...a.a..", 7, 2)
  for i 0 NB
    if bon[i] == 1
      rect(bxs[i], bys[i], 2, 6, 14)
  if lives > 0 and hurt % 6 < 3
    sprite(px, 172, "...f.....bfb...bbbbb.bb.b.bbb..e..b", 7, 2)
  text(4, 2, "SCORE", 15)
  num(52, 2, score, 15)
  text(120, 2, "WAVE", 15)
  num(160, 2, wave, 15)
  text(210, 2, "LIVES", 15)
  num(258, 2, lives, 15)
  text(4, 190, "BEST", 7)
  num(40, 190, best, 7)
  if lives == 0
    rect(60, 70, 200, 60, 1)
    rect(60, 70, 200, 1, 15)
    rect(60, 129, 200, 1, 15)
    text(124, 80, "GAME OVER", 14)
    text(84, 100, "A retry     ESC menu", 15)
    gate = gate + 1
    if key(4) and gate > 40
      gate = 0
      lives = 3
      score = 0
      wave = 1
      for i 0 NE
        eon[i] = 0
  frame()
