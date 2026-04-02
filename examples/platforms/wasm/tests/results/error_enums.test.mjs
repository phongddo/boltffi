import {
  assert,
  assertThrowsWithCode,
  demo,
} from "../support/index.mjs";

export async function run() {
  assert.equal(demo.checkedDivide(10, 2), 5);
  assertThrowsWithCode(
    () => demo.checkedDivide(1, 0),
    demo.MathErrorException,
    demo.MathError.DivisionByZero,
  );
  assert.equal(demo.checkedSqrt(9), 3);
  assertThrowsWithCode(
    () => demo.checkedSqrt(-1),
    demo.MathErrorException,
    demo.MathError.NegativeInput,
  );
  assertThrowsWithCode(
    () => demo.checkedAdd(2_147_483_647, 1),
    demo.MathErrorException,
    demo.MathError.Overflow,
  );
  assert.equal(demo.validateUsername("valid_name"), "valid_name");
  assertThrowsWithCode(
    () => demo.validateUsername("ab"),
    demo.ValidationErrorException,
    demo.ValidationError.TooShort,
  );
  assertThrowsWithCode(
    () => demo.validateUsername("a".repeat(21)),
    demo.ValidationErrorException,
    demo.ValidationError.TooLong,
  );
  assertThrowsWithCode(
    () => demo.validateUsername("has space"),
    demo.ValidationErrorException,
    demo.ValidationError.InvalidFormat,
  );

  assert.equal(demo.mayFail(true), "Success!");
  assert.equal(demo.divideApp(10, 2), 5);
}
