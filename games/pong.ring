py = 80
cy = 80
bx = 160
by = 100
vx = 3
vy = 2
ps = 0
cs = 0
best = load(0)
saved = 0
heart 1 == 1
  clear(0)
  if cs < 5
    if key(2)
      py = py - 4
    if key(3)
      py = py + 4
    if py < 12
      py = 12
    if py > 148
      py = 148
    if by > cy + 26
      cy = cy + 3
    if by < cy + 14
      cy = cy - 3
    if cy < 12
      cy = 12
    if cy > 148
      cy = 148
    bx = bx + vx
    by = by + vy
    if by < 12
      vy = 2
    if by > 190
      vy = -2
    if vx < 0 and bx <= 14 and bx >= 2 and by + 6 > py and by < py + 40
      vx = 3
      vy = (by + 3 - py - 20) / 6
      beep(660, 30)
    if vx > 0 and bx + 6 >= 306 and bx + 6 <= 316 and by + 6 > cy and by < cy + 40
      vx = -3
      vy = (by + 3 - cy - 20) / 6
      beep(520, 30)
    if bx < -6
      cs = cs + 1
      bx = 160
      by = 100
      vx = 3
      vy = 2
      beep(150, 200)
    if bx > 320
      ps = ps + 1
      bx = 160
      by = 100
      vx = -3
      vy = 2
      beep(880, 100)
  rect(159, 12, 2, 188, 8)
  rect(8, py, 6, 40, 15)
  rect(306, cy, 6, 40, 15)
  rect(bx, by, 6, 6, 14)
  text(4, 2, "PONG", 10)
  num(100, 2, ps, 15)
  num(210, 2, cs, 15)
  text(240, 2, "BEST", 7)
  num(280, 2, best, 7)
  if cs >= 5
    rect(60, 70, 200, 60, 1)
    rect(60, 70, 200, 1, 15)
    rect(60, 129, 200, 1, 15)
    text(124, 80, "GAME OVER", 14)
    text(84, 100, "A retry     ESC menu", 15)
    if saved == 0
      saved = 1
      if ps > best
        best = ps
        save(0, best)
    if key(4)
      saved = 0
      cs = 0
      ps = 0
  frame()
