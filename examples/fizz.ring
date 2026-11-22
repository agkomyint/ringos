i = 1
heart i <= 15
  if i % 15 == 0
    mouth "FizzBuzz"
  else if i % 3 == 0
    mouth "Fizz"
  else if i % 5 == 0
    mouth "Buzz"
  else
    mouth i
  i = i + 1
