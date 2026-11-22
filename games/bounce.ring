x = 160
y = 60
dx = 3
dy = 2
px = 140
score = 0
best = load(0)
heart 1 == 1
  clear(0)
  if key(0)
    px = px - 5
  if key(1)
    px = px + 5
  if px < 0
    px = 0
  if px > 280
    px = 280
  x = x + dx
  y = y + dy
  if x < 0
    dx = 3
  if x > 308
    dx = -3
  if y < 14
    dy = 2
  if y + 12 >= 186 and y + 12 <= 194 and x + 12 > px and x < px + 40
    dy = -2
    score = score + 1
    beep(660, 30)
    if score > best
      best = score
      save(0, best)
  if y > 200
    x = 160
    y = 60
    dy = 2
    score = 0
    beep(150, 200)
  rect(0, 12, 320, 1, 8)
  rect(x, y, 12, 12, 14)
  rect(px, 190, 40, 6, 11)
  text(4, 2, "BOUNCE", 10)
  num(100, 2, score, 15)
  text(200, 2, "BEST", 15)
  num(240, 2, best, 15)
  frame()
