by = 90
vy = 0
px = 320
gy = 50
score = 0
over = 0
held = 0
t = 0
best = load(0)
saved = 0
heart 1 == 1
  clear(11)
  if over == 0
    t = t + 1
    if key(4) and held == 0
      vy = -5
      held = 1
      beep(700, 25)
    if key(4) == 0
      held = 0
    if t % 3 == 0 and vy < 5
      vy = vy + 1
    by = by + vy
    px = px - 2
    if px < -24
      px = 320
      gy = 30 + rnd(100)
      score = score + 1
      beep(990, 60)
    if by < 0 or by > 188
      over = 1
    if 52 > px and 40 < px + 24
      if by < gy or by + 12 > gy + 60
        over = 1
  rect(px, 0, 24, gy, 2)
  rect(px, gy + 60, 24, 200, 2)
  rect(40, by, 12, 12, 14)
  text(4, 4, "SCORE", 0)
  num(52, 4, score, 0)
  text(200, 4, "BEST", 0)
  num(240, 4, best, 0)
  if over == 1
    text(72, 90, "GAME OVER - A to retry", 0)
    if saved == 0
      saved = 1
      beep(150, 300)
      if score > best
        best = score
        save(0, best)
    if key(4)
      saved = 0
      over = 0
      by = 90
      vy = 0
      px = 320
      score = 0
  frame()
