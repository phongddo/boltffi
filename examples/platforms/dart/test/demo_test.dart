import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  test("demo", () {
    expect($$Native.boltffi_echo_bool(true), equals(true));
    expect($$Native.boltffi_echo_bool(false), equals(false));
  });
}
