tx = new 600
ty = new 600
len = 3
tx[0] = 10
ty[0] = 9
tx[1] = 9
ty[1] = 9
tx[2] = 8
ty[2] = 9
dx = 1
dy = 0
fx = rnd(32)
fy = rnd(18)
score = 0
over = 0
tick = 0
heart 1 == 1
  clear(0)
  if over == 0
    if key(0) and dx == 0
      dx = -1
      dy = 0
    if key(1) and dx == 0
      dx = 1
      dy = 0
    if key(2) and dy == 0
      dx = 0
      dy = -1
    if key(3) and dy == 0
      dx = 0
      dy = 1
    tick = tick + 1
    if tick >= 6
      tick = 0
      nx = tx[0] + dx
      ny = ty[0] + dy
      if nx < 0 or nx >= 32 or ny < 0 or ny >= 18
        over = 1
      i = 0
      heart i < len
        if tx[i] == nx and ty[i] == ny
          over = 1
        i = i + 1
      if over == 0
        if nx == fx and ny == fy
          len = len + 1
          score = score + 1
          fx = rnd(32)
          fy = rnd(18)
        i = len - 1
        heart i > 0
          tx[i] = tx[i - 1]
          ty[i] = ty[i - 1]
          i = i - 1
        tx[0] = nx
        ty[0] = ny
  i = 0
  heart i < len
    rect(tx[i] * 10, 20 + ty[i] * 10, 9, 9, 10)
    i = i + 1
  rect(fx * 10, 20 + fy * 10, 9, 9, 12)
  text(4, 6, "SNAKE", 14)
  num(60, 6, score, 15)
  if over == 1
    text(72, 90, "GAME OVER - A to retry", 15)
    if key(4)
      len = 3
      tx[0] = 10
      ty[0] = 9
      tx[1] = 9
      ty[1] = 9
      tx[2] = 8
      ty[2] = 9
      dx = 1
      dy = 0
      score = 0
      over = 0
      tick = 0
  frame()
