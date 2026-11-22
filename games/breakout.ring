gate = 0
br = new 50
i = 0
heart i < 50
  br[i] = 1
  i = i + 1
px = 140
bx = 160
by = 120
vx = 2
vy = -3
score = 0
lives = 3
left = 50
best = load(0)
saved = 0
heart 1 == 1
  clear(0)
  if lives > 0 and left > 0
    if key(0)
      px = px - 5
    if key(1)
      px = px + 5
    if px < 0
      px = 0
    if px > 280
      px = 280
    bx = bx + vx
    by = by + vy
    if bx < 0
      vx = 2
    if bx > 314
      vx = -2
    if by < 12
      vy = 3
    if vy > 0 and by + 6 >= 186 and by + 6 <= 192 and bx + 6 > px and bx < px + 40
      vy = -3
      vx = (bx + 3 - px - 20) / 6
      if vx == 0
        vx = 1
      beep(660, 30)
    col = (bx + 3 - 10) / 30
    row = (by + 3 - 30) / 12
    if bx + 3 >= 10 and by + 3 >= 30 and col < 10 and row < 5
      if br[row * 10 + col] == 1
        br[row * 10 + col] = 0
        vy = -vy
        score = score + 10
        left = left - 1
        beep(880, 30)
    if by > 200
      lives = lives - 1
      bx = 160
      by = 120
      vx = 2
      vy = -3
      beep(150, 250)
  i = 0
  heart i < 50
    if br[i] == 1
      rect(10 + (i % 10) * 30, 30 + (i / 10) * 12, 28, 10, 9 + i / 10)
    i = i + 1
  rect(px, 190, 40, 6, 11)
  rect(bx, by, 6, 6, 14)
  text(4, 2, "SCORE", 15)
  num(52, 2, score, 15)
  text(120, 2, "LIVES", 15)
  num(168, 2, lives, 15)
  text(220, 2, "BEST", 7)
  num(260, 2, best, 7)
  if lives == 0 or left == 0
    if left == 0
      rect(60, 70, 200, 60, 1)
      rect(60, 70, 200, 1, 15)
      rect(60, 129, 200, 1, 15)
      text(132, 80, "YOU WIN", 14)
      text(84, 100, "A retry     ESC menu", 15)
    else
      rect(60, 70, 200, 60, 1)
      rect(60, 70, 200, 1, 15)
      rect(60, 129, 200, 1, 15)
      text(124, 80, "GAME OVER", 14)
      text(84, 100, "A retry     ESC menu", 15)
    gate = gate + 1
    if saved == 0
      saved = 1
      if score > best
        best = score
        save(0, best)
    if key(4) and gate > 40
      gate = 0
      saved = 0
      i = 0
      heart i < 50
        br[i] = 1
        i = i + 1
      px = 140
      bx = 160
      by = 120
      vx = 2
      vy = -3
      score = 0
      lives = 3
      left = 50
  frame()
