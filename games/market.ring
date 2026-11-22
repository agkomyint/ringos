# Market: a tiny stock exchange. Prices are fixed point (x1000), moved by bell-curve noise.
# Left/Right pick a stock, A buys one share, B sells one, Up/Down change the speed.
const N 4
const H 60
global price
global hs
global cash
global held

step
  for i 0 N
    p = price[i]
    p = p + pct(p, rndn(100, 400 + i * 300)) / 100
    if p < 1000
      p = 1000
    price[i] = p
    a = hs[i]
    for j 1 H
      a[j - 1] = a[j]
    a[H - 1] = p

worth
  w = cash
  for i 0 N
    w = w + fmul(held[i], price[i])
  back w

price = new 4
hs = new 4
held = new 4
cash = 1000000
for i 0 N
  price[i] = 20000 + i * 15000
  a = new 60
  fill(a, H, price[i])
  hs[i] = a

sel = 0
hz = 4
best = load(0)
a_was = 0
b_was = 0
l_was = 0
r_was = 0
u_was = 0
d_was = 0
seed(12345)
heart 1 == 1
  clear(0)
  n = tick(hz)
  for t 0 n
    step()
  if key(0) and l_was == 0
    sel = (sel + N - 1) % N
  if key(1) and r_was == 0
    sel = (sel + 1) % N
  if key(2) and u_was == 0
    hz = min(hz + 2, 30)
  if key(3) and d_was == 0
    hz = max(hz - 2, 1)
  if key(4) and a_was == 0 and cash >= price[sel]
    cash = cash - price[sel]
    held[sel] = held[sel] + 1000
  if key(5) and b_was == 0 and held[sel] >= 1000
    cash = cash + price[sel]
    held[sel] = held[sel] - 1000
  l_was = key(0)
  r_was = key(1)
  u_was = key(2)
  d_was = key(3)
  a_was = key(4)
  b_was = key(5)
  textx(8, 6, "MARKET", 14, 2)
  text(8, 26, "CASH", 7)
  num(48, 26, cash / 1000, 15)
  text(120, 26, "WORTH", 7)
  w = worth()
  num(170, 26, w / 1000, 15)
  if w > best
    best = w
    save(0, best)
  text(230, 26, "BEST", 7)
  num(264, 26, best / 1000, 6)
  for i 0 N
    y = 44 + i * 36
    c = 8
    if i == sel
      c = 14
      rect(4, y - 2, 312, 34, 1)
    text(8, y, "STK", c)
    num(36, y, i + 1, c)
    num(60, y, price[i] / 1000, 15)
    text(60, y + 10, "HELD", 7)
    num(92, y + 10, held[i] / 1000, 15)
    a = hs[i]
    col = 10
    if a[H - 1] < a[0]
      col = 12
    plot(a, H, 130, y, 180, 26, col)
  text(8, 190, "A BUY  B SELL  UP DOWN SPEED", 7)
  frame()
