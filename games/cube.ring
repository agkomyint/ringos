# Cube: software 3D. Eight points are turned, pushed through a perspective divide and joined
# with line(). Arrows spin it, A stops it, B resets.
xs = new 8
ys = new 8
zs = new 8
px = new 8
py = new 8
for i 0 4
  x = 1000
  if i == 0 or i == 3
    x = -1000
  y = 1000
  if i < 2
    y = -1000
  xs[i] = x
  xs[i + 4] = x
  ys[i] = y
  ys[i + 4] = y
  zs[i] = -1000
  zs[i + 4] = 1000

ax = 20
ay = 0
vx = 1
vy = 2
l_was = 0
r_was = 0
u_was = 0
d_was = 0
heart 1 == 1
  clear(0)
  if key(0) and l_was == 0
    vy = vy - 1
  if key(1) and r_was == 0
    vy = vy + 1
  if key(2) and u_was == 0
    vx = vx - 1
  if key(3) and d_was == 0
    vx = vx + 1
  l_was = key(0)
  r_was = key(1)
  u_was = key(2)
  d_was = key(3)
  if key(4)
    vx = 0
    vy = 0
  if key(5)
    vx = 1
    vy = 2
    ax = 20
    ay = 0
  ax = (ax + vx + 360) % 360
  ay = (ay + vy + 360) % 360
  sx = sin(ax)
  cx = cos(ax)
  sy = sin(ay)
  cy = cos(ay)
  for i 0 8
    x = xs[i]
    y = ys[i]
    z = zs[i]
    x2 = fmul(x, cy) + fmul(z, sy)
    z2 = fmul(z, cy) - fmul(x, sy)
    y2 = fmul(y, cx) - fmul(z2, sx)
    z3 = fmul(y, sx) + fmul(z2, cx)
    d = z3 + 5500
    px[i] = 160 + x2 * 260 / d
    py[i] = 100 + y2 * 260 / d
  for i 0 4
    j = (i + 1) % 4
    line(px[i], py[i], px[j], py[j], 14)
    line(px[i + 4], py[i + 4], px[j + 4], py[j + 4], 9)
    line(px[i], py[i], px[i + 4], py[i + 4], 11)
  textx(8, 6, "CUBE", 14, 2)
  text(8, 184, "ARROWS SPIN  A STOP  B RESET", 7)
  frame()
