# Lemonade: run a stand for 30 days. Hot days bring customers, high prices scare them off.
# Left/Right pick a setting, Up/Down change it, A (or Enter) opens the stand for the day.
const DAYS 30
global cash
global day
global price
global cups
global ads
global temp
global rain
global sold
global cust
global profit
global hist
global over

digits n
  d = 1
  m = n
  heart m >= 10
    m = m / 10
    d = d + 1
  back d

newday
  temp = max(5, min(40, rndn(24, 8)))
  rain = 0
  if rnd(100) < 25
    rain = 1

reset
  cash = 2000
  day = 1
  price = 60
  cups = 20
  ads = 0
  sold = 0
  cust = 0
  profit = 0
  over = 0
  fill(hist, DAYS + 1, 0)
  hist[0] = cash
  newday()

runday
  c = max(5, 20 + rndn(0, 3))
  ads = min(ads, cash)
  cups = min(cups, (cash - ads) / c)
  base = temp + ads / 100
  willing = base * max(0, 300 - price) / 150
  if rain == 1
    willing = willing / 2
  cust = max(0, willing + rndn(0, 4))
  sold = min(cups, cust)
  profit = sold * price - cups * c - ads
  cash = cash + profit
  hist[day] = cash
  day = day + 1
  if day > DAYS or cash < 20
    over = 1
  else
    newday()

money x y v c
  text(x, y, "$", c)
  num(x + 8, y, v / 100, c)

hist = new 31
reset()
sel = 0
best = load(0)
saved = 0
gate = 0
l_was = 0
r_was = 0
u_was = 0
d_was = 0
a_was = 0
heart 1 == 1
  clear(0)
  if over == 0
    if key(0) and l_was == 0
      sel = (sel + 2) % 3
    if key(1) and r_was == 0
      sel = (sel + 1) % 3
    step = 0
    if key(2) and u_was == 0
      step = 1
    if key(3) and d_was == 0
      step = -1
    if sel == 0
      price = max(5, min(400, price + step * 5))
    if sel == 1
      cups = max(0, min(200, cups + step * 5))
    if sel == 2
      ads = max(0, min(1000, ads + step * 50))
    if (key(4) or key(6)) and a_was == 0
      runday()
    l_was = key(0)
    r_was = key(1)
    u_was = key(2)
    d_was = key(3)
    a_was = key(4) or key(6)
    textx(8, 6, "LEMONADE", 14, 2)
    text(8, 28, "DAY", 7)
    num(36, 28, min(day, DAYS), 15)
    text(60, 28, "OF 30", 7)
    text(8, 42, "TODAY", 7)
    num(56, 42, temp, 15)
    text(56 + 8 * digits(temp), 42, "C", 15)
    if rain == 1
      text(8, 54, "RAINY: FEWER BUYERS", 12)
    else
      text(8, 54, "DRY", 10)
    text(8, 70, "CASH", 7)
    money(48, 70, cash, 15)
    c0 = 8
    if sel == 0
      c0 = 14
      rect(4, 85, 112, 12, 1)
    text(8, 87, "PRICE", c0)
    num(64, 87, price, 15)
    text(64 + 8 * digits(price), 87, "C", 15)
    c1 = 8
    if sel == 1
      c1 = 14
      rect(4, 101, 112, 12, 1)
    text(8, 103, "CUPS", c1)
    num(64, 103, cups, 15)
    c2 = 8
    if sel == 2
      c2 = 14
      rect(4, 117, 112, 12, 1)
    text(8, 119, "ADS", c2)
    money(64, 119, ads, 15)
    if day > 1
      text(8, 140, "LAST DAY", 7)
      text(8, 152, "SOLD", 7)
      num(48, 152, sold, 15)
      text(56 + 8 * digits(sold), 152, "OF", 7)
      num(84 + 8 * digits(sold), 152, cust, 15)
      text(8, 164, "PROFIT", 7)
      if profit >= 0
        money(64, 164, profit, 10)
      else
        money(64, 164, 0 - profit, 12)
        text(56, 164, "-", 12)
    text(130, 28, "CASH OVER THE DAYS", 7)
    if day >= 2
      plot(hist, day, 130, 44, 180, 100, 10)
    text(8, 190, "A OPENS THE STAND  UP DOWN CHANGE", 7)
  else
    if saved == 0
      saved = 1
      gate = 0
      if cash > best
        best = cash
        save(0, best)
    gate = gate + 1
    textx(60, 40, "STAND CLOSED", 14, 2)
    text(90, 90, "FINAL CASH", 7)
    money(190, 90, cash, 15)
    text(90, 106, "BEST", 7)
    money(190, 106, best, 6)
    if gate > 40
      text(80, 150, "PRESS A TO PLAY AGAIN", 10)
      if key(4) or key(6)
        reset()
        saved = 0
        a_was = 1
  frame()
