hit px bx by
  if bx + 12 > px and bx < px + 24 and by + 12 > 180
    back 1
  back 0

px = 148
bx = rnd(300)
by = 0
score = 0
over = 0
speed = 2
best = load(0)
saved = 0
heart 1 == 1
  clear(1)
  if over == 0
    if key(0)
      px = px - 4
    if key(1)
      px = px + 4
    if px < 0
      px = 0
    if px > 296
      px = 296
    by = by + speed
    if hit(px, bx, by) == 1
      over = 1
    if by > 200
      by = 0
      bx = rnd(308)
      score = score + 1
      beep(880, 40)
      speed = 2 + score / 5
  rect(px, 180, 24, 8, 14)
  rect(bx, by, 12, 12, 12)
  text(4, 4, "SCORE", 15)
  num(52, 4, score, 15)
  text(200, 4, "BEST", 15)
  num(240, 4, best, 15)
  if over == 1
    rect(60, 70, 200, 60, 1)
    rect(60, 70, 200, 1, 15)
    rect(60, 129, 200, 1, 15)
    text(124, 80, "GAME OVER", 14)
    text(84, 100, "A retry     ESC menu", 15)
    if saved == 0
      saved = 1
      beep(150, 300)
      if score > best
        best = score
        save(0, best)
    if key(4)
      saved = 0
      px = 148
      bx = rnd(300)
      by = 0
      score = 0
      over = 0
      speed = 2
  frame()
