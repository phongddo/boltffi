import com.boltffi.demo.Demo;

public class DemoTest {
    public static void main(String[] args) {
        System.out.println("Testing Java bindings...\n");

        System.out.println("Testing bool...");
        assert Demo.echoBool(true) == true : "echoBool(true)";
        assert Demo.echoBool(false) == false : "echoBool(false)";
        assert Demo.negateBool(true) == false : "negateBool(true)";
        assert Demo.negateBool(false) == true : "negateBool(false)";
        System.out.println("  PASS\n");

        System.out.println("Testing i32...");
        assert Demo.echoI32(42) == 42 : "echoI32(42)";
        assert Demo.echoI32(-100) == -100 : "echoI32(-100)";
        assert Demo.addI32(10, 20) == 30 : "addI32(10, 20)";
        System.out.println("  PASS\n");

        System.out.println("Testing i64...");
        assert Demo.echoI64(9999999999L) == 9999999999L : "echoI64(large)";
        assert Demo.echoI64(-9999999999L) == -9999999999L : "echoI64(negative large)";
        System.out.println("  PASS\n");

        System.out.println("Testing f32...");
        assert Math.abs(Demo.echoF32(3.14f) - 3.14f) < 0.001f : "echoF32(3.14)";
        assert Math.abs(Demo.addF32(1.5f, 2.5f) - 4.0f) < 0.001f : "addF32(1.5, 2.5)";
        System.out.println("  PASS\n");

        System.out.println("Testing f64...");
        assert Math.abs(Demo.echoF64(3.14159265359) - 3.14159265359) < 0.0000001 : "echoF64(pi)";
        assert Math.abs(Demo.addF64(1.5, 2.5) - 4.0) < 0.0000001 : "addF64(1.5, 2.5)";
        System.out.println("  PASS\n");

        System.out.println("Testing strings...");
        assert Demo.echoString("hello").equals("hello") : "echoString(hello)";
        assert Demo.echoString("").equals("") : "echoString(empty)";
        assert Demo.echoString("café").equals("café") : "echoString(unicode)";
        assert Demo.echoString("日本語").equals("日本語") : "echoString(cjk)";
        assert Demo.echoString("hello 🌍 world").equals("hello 🌍 world") : "echoString(emoji)";
        assert Demo.concatStrings("foo", "bar").equals("foobar") : "concatStrings(foo, bar)";
        assert Demo.concatStrings("", "bar").equals("bar") : "concatStrings(empty, bar)";
        assert Demo.concatStrings("foo", "").equals("foo") : "concatStrings(foo, empty)";
        assert Demo.concatStrings("🎉", "🎊").equals("🎉🎊") : "concatStrings(emoji)";
        assert Demo.stringLength("hello") == 5 : "stringLength(hello)";
        assert Demo.stringLength("") == 0 : "stringLength(empty)";
        assert Demo.stringLength("café") == 5 : "stringLength(utf8 bytes)";
        assert Demo.stringLength("🌍") == 4 : "stringLength(emoji 4 bytes)";
        System.out.println("  PASS\n");

        System.out.println("All tests passed!");
    }
}
