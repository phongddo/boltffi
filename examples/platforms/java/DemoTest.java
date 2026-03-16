package com.boltffi.demo;

import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Optional;

public final class DemoTest {
    public static void main(String[] args) {
        System.out.println("Testing Java bindings...\n");
        testBool();
        testI32();
        testI64();
        testF32();
        testF64();
        testStrings();
        testPointRecords();
        testLineRecords();
        testPersonRecords();
        testCStyleEnums();
        testDataEnums();
        testCStyleEnumVecs();
        testDataEnumVecs();
        testBytesVecs();
        testPrimitiveVecs();
        testVecStrings();
        testOptions();
        testRecordsWithVecs();
        System.out.println("All tests passed!");
    }

    private static void testBool() {
        System.out.println("Testing bool...");
        assert Demo.echoBool(true);
        assert !Demo.echoBool(false);
        assert !Demo.negateBool(true);
        assert Demo.negateBool(false);
        System.out.println("  PASS\n");
    }

    private static void testI32() {
        System.out.println("Testing i32...");
        assert Demo.echoI32(42) == 42 : "echoI32(42)";
        assert Demo.echoI32(-100) == -100 : "echoI32(-100)";
        assert Demo.addI32(10, 20) == 30 : "addI32(10, 20)";
        System.out.println("  PASS\n");
    }

    private static void testI64() {
        System.out.println("Testing i64...");
        assert Demo.echoI64(9999999999L) == 9999999999L : "echoI64(large)";
        assert Demo.echoI64(-9999999999L) == -9999999999L : "echoI64(negative large)";
        System.out.println("  PASS\n");
    }

    private static void testF32() {
        System.out.println("Testing f32...");
        assert Math.abs(Demo.echoF32(3.14f) - 3.14f) < 0.001f : "echoF32(3.14)";
        assert Math.abs(Demo.addF32(1.5f, 2.5f) - 4.0f) < 0.001f : "addF32(1.5, 2.5)";
        System.out.println("  PASS\n");
    }

    private static void testF64() {
        System.out.println("Testing f64...");
        assert Math.abs(Demo.echoF64(3.14159265359) - 3.14159265359) < 0.0000001 : "echoF64(pi)";
        assert Math.abs(Demo.addF64(1.5, 2.5) - 4.0) < 0.0000001 : "addF64(1.5, 2.5)";
        System.out.println("  PASS\n");
    }

    private static void testStrings() {
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
    }

    private static void testPointRecords() {
        System.out.println("Testing records (Point)...");
        Point point = Demo.makePoint(1.0, 2.0);
        assert point.x() == 1.0 : "makePoint.x";
        assert point.y() == 2.0 : "makePoint.y";
        Point echoedPoint = Demo.echoPoint(point);
        assert echoedPoint.x() == 1.0 : "echoPoint.x";
        assert echoedPoint.y() == 2.0 : "echoPoint.y";
        Point sumPoint = Demo.addPoints(new Point(3.0, 4.0), new Point(5.0, 6.0));
        assert sumPoint.x() == 8.0 : "addPoints.x";
        assert sumPoint.y() == 10.0 : "addPoints.y";
        assert Math.abs(Demo.pointDistance(new Point(3.0, 4.0)) - 5.0) < 0.0001 : "pointDistance";
        System.out.println("  PASS\n");
    }

    private static void testLineRecords() {
        System.out.println("Testing records (Line)...");
        Line line = Demo.makeLine(0.0, 0.0, 3.0, 4.0);
        assert line.start().x() == 0.0 : "makeLine.start.x";
        assert line.end().y() == 4.0 : "makeLine.end.y";
        Line echoedLine = Demo.echoLine(line);
        assert echoedLine.start().x() == 0.0 : "echoLine.start.x";
        assert echoedLine.end().x() == 3.0 : "echoLine.end.x";
        assert Math.abs(Demo.lineLength(line) - 5.0) < 0.0001 : "lineLength";
        System.out.println("  PASS\n");
    }

    private static void testPersonRecords() {
        System.out.println("Testing records (Person)...");
        Person person = Demo.makePerson("Alice", 30);
        assert person.name().equals("Alice") : "makePerson.name";
        assert person.age() == 30 : "makePerson.age";
        Person echoedPerson = Demo.echoPerson(person);
        assert echoedPerson.name().equals("Alice") : "echoPerson.name";
        assert echoedPerson.age() == 30 : "echoPerson.age";
        assert Demo.greetPerson(person).equals("Hello, Alice! You are 30 years old.") : "greetPerson";
        Person emojiPerson = Demo.makePerson("🎉 Party", 25);
        assert emojiPerson.name().equals("🎉 Party") : "makePerson(emoji)";
        Person echoedEmojiPerson = Demo.echoPerson(emojiPerson);
        assert echoedEmojiPerson.name().equals("🎉 Party") : "echoPerson(emoji)";
        System.out.println("  PASS\n");
    }

    private static void testCStyleEnums() {
        System.out.println("Testing C-style enums...");

        assert Demo.echoStatus(Status.ACTIVE) == Status.ACTIVE : "echoStatus(Active)";
        assert Demo.echoStatus(Status.INACTIVE) == Status.INACTIVE : "echoStatus(Inactive)";
        assert Demo.echoStatus(Status.PENDING) == Status.PENDING : "echoStatus(Pending)";
        assert Demo.statusToString(Status.ACTIVE).equals("active") : "statusToString(Active)";
        assert Demo.statusToString(Status.INACTIVE).equals("inactive") : "statusToString(Inactive)";
        assert Demo.isActive(Status.ACTIVE) : "isActive(Active)";
        assert !Demo.isActive(Status.PENDING) : "isActive(Pending)";

        assert Demo.echoDirection(Direction.NORTH) == Direction.NORTH : "echoDirection(North)";
        assert Demo.oppositeDirection(Direction.NORTH) == Direction.SOUTH : "oppositeDirection(North)";
        assert Demo.oppositeDirection(Direction.EAST) == Direction.WEST : "oppositeDirection(East)";

        assert Demo.echoPriority(Priority.HIGH) == Priority.HIGH : "echoPriority(High)";
        assert Demo.priorityLabel(Priority.LOW).equals("low") : "priorityLabel(Low)";
        assert Demo.isHighPriority(Priority.CRITICAL) : "isHighPriority(Critical)";
        assert !Demo.isHighPriority(Priority.LOW) : "isHighPriority(Low)";

        assert Demo.echoLogLevel(LogLevel.INFO) == LogLevel.INFO : "echoLogLevel(Info)";
        assert Demo.shouldLog(LogLevel.ERROR, LogLevel.WARN) : "shouldLog(Error >= Warn)";
        assert !Demo.shouldLog(LogLevel.DEBUG, LogLevel.INFO) : "shouldLog(Debug < Info)";

        System.out.println("  PASS\n");
    }

    private static void testDataEnums() {
        System.out.println("Testing data enums...");

        Shape circle = Demo.makeCircle(5.0);
        assert circle instanceof Shape.Circle : "makeCircle returns Circle";
        assert ((Shape.Circle) circle).radius == 5.0 : "makeCircle.radius";
        assert Math.abs(Demo.shapeArea(circle) - Math.PI * 25.0) < 0.0001 : "shapeArea(circle)";

        Shape rect = Demo.makeRectangle(3.0, 4.0);
        assert rect instanceof Shape.Rectangle : "makeRectangle returns Rectangle";
        assert Math.abs(Demo.shapeArea(rect) - 12.0) < 0.0001 : "shapeArea(rect)";

        Shape echoedCircle = Demo.echoShape(circle);
        assert echoedCircle instanceof Shape.Circle : "echoShape(circle) type";
        assert ((Shape.Circle) echoedCircle).radius == 5.0 : "echoShape(circle).radius";

        Shape echoedRect = Demo.echoShape(rect);
        assert echoedRect instanceof Shape.Rectangle : "echoShape(rect) type";

        Shape triangle = Demo.echoShape(new Shape.Triangle(
            new Point(0.0, 0.0), new Point(3.0, 0.0), new Point(0.0, 4.0)
        ));
        assert triangle instanceof Shape.Triangle : "echoShape(triangle) type";
        assert Math.abs(Demo.shapeArea(triangle) - 6.0) < 0.0001 : "shapeArea(triangle)";

        Shape point = Demo.echoShape(Shape.Point.INSTANCE);
        assert point instanceof Shape.Point : "echoShape(point) type";
        assert Demo.shapeArea(point) == 0.0 : "shapeArea(point)";

        Message text = Demo.echoMessage(new Message.Text("hello"));
        assert text instanceof Message.Text : "echoMessage(Text) type";
        assert ((Message.Text) text).body.equals("hello") : "echoMessage(Text).body";
        assert Demo.messageSummary(new Message.Text("hi")).equals("text: hi") : "messageSummary(Text)";
        assert Demo.messageSummary(Message.Ping.INSTANCE).equals("ping") : "messageSummary(Ping)";

        Animal dog = Demo.echoAnimal(new Animal.Dog("Rex", "Labrador"));
        assert dog instanceof Animal.Dog : "echoAnimal(Dog) type";
        assert ((Animal.Dog) dog).name.equals("Rex") : "echoAnimal(Dog).name";
        assert Demo.animalName(new Animal.Fish(5)).equals("5 fish") : "animalName(Fish)";

        ApiResponse success = Demo.echoApiResponse(new ApiResponse.Success("ok"));
        assert success instanceof ApiResponse.Success : "echoApiResponse(Success) type";
        assert Demo.isSuccess(new ApiResponse.Success("data")) : "isSuccess(Success)";
        assert !Demo.isSuccess(ApiResponse.Empty.INSTANCE) : "isSuccess(Empty)";

        System.out.println("  PASS\n");
    }

    private static void testCStyleEnumVecs() {
        System.out.println("Testing vec C-style enums...");

        List<Status> statuses = Demo.echoVecStatus(
            Arrays.asList(Status.ACTIVE, Status.PENDING, Status.INACTIVE)
        );
        assert statuses.size() == 3 : "echoVecStatus size";
        assert statuses.get(0) == Status.ACTIVE : "echoVecStatus[0]";
        assert statuses.get(1) == Status.PENDING : "echoVecStatus[1]";
        assert statuses.get(2) == Status.INACTIVE : "echoVecStatus[2]";

        List<LogLevel> levels = Demo.echoVecLogLevel(
            Arrays.asList(LogLevel.TRACE, LogLevel.INFO, LogLevel.ERROR)
        );
        assert levels.size() == 3 : "echoVecLogLevel size";
        assert levels.get(0) == LogLevel.TRACE : "echoVecLogLevel[0]";
        assert levels.get(1) == LogLevel.INFO : "echoVecLogLevel[1]";
        assert levels.get(2) == LogLevel.ERROR : "echoVecLogLevel[2]";

        System.out.println("  PASS\n");
    }

    private static void testDataEnumVecs() {
        System.out.println("Testing vec data enums...");

        List<Shape> shapes = Demo.echoVecShape(Arrays.asList(
            new Shape.Circle(2.0),
            new Shape.Rectangle(3.0, 4.0),
            Shape.Point.INSTANCE
        ));

        assert shapes.size() == 3 : "echoVecShape size";
        assert shapes.get(0) instanceof Shape.Circle : "echoVecShape[0] type";
        assert Math.abs(((Shape.Circle) shapes.get(0)).radius - 2.0) < 0.0001 : "echoVecShape[0].radius";
        assert shapes.get(1) instanceof Shape.Rectangle : "echoVecShape[1] type";
        assert shapes.get(2) instanceof Shape.Point : "echoVecShape[2] type";

        System.out.println("  PASS\n");
    }

    private static void testBytesVecs() {
        System.out.println("Testing vec bytes...\n");

        byte[] echoed = Demo.echoBytes(new byte[]{1, 2, 3, 4});
        assert echoed.length == 4 : "echoBytes length";
        assert echoed[0] == 1 && echoed[3] == 4 : "echoBytes values";

        assert Demo.bytesLength(new byte[]{10, 20, 30}) == 3 : "bytesLength";
        assert Demo.bytesSum(new byte[]{1, 2, 3, 4}) == 10 : "bytesSum";

        byte[] made = Demo.makeBytes(5);
        assert made.length == 5 : "makeBytes length";
        assert made[0] == 0 && made[4] == 4 : "makeBytes values";

        byte[] reversed = Demo.reverseBytes(new byte[]{5, 6, 7});
        assert reversed.length == 3 : "reverseBytes length";
        assert reversed[0] == 7 && reversed[2] == 5 : "reverseBytes values";

        System.out.println("  PASS\n");
    }

    private static void testPrimitiveVecs() {
        System.out.println("Testing primitive vecs...");

        int[] ints = Demo.echoVecI32(new int[]{1, 2, 3});
        assert ints.length == 3 : "echoVecI32 length";
        assert ints[0] == 1 && ints[1] == 2 && ints[2] == 3 : "echoVecI32 values";

        int[] empty = Demo.echoVecI32(new int[0]);
        assert empty.length == 0 : "echoVecI32 empty";

        assert Demo.sumVecI32(new int[]{10, 20, 30}) == 60L : "sumVecI32";
        assert Demo.sumVecI32(new int[0]) == 0L : "sumVecI32 empty";

        double[] doubles = Demo.echoVecF64(new double[]{1.5, 2.5});
        assert doubles.length == 2 : "echoVecF64 length";
        assert Math.abs(doubles[0] - 1.5) < 0.0001 : "echoVecF64[0]";
        assert Math.abs(doubles[1] - 2.5) < 0.0001 : "echoVecF64[1]";

        boolean[] bools = Demo.echoVecBool(new boolean[]{true, false, true});
        assert bools.length == 3 : "echoVecBool length";
        assert bools[0] && !bools[1] && bools[2] : "echoVecBool values";

        byte[] i8s = Demo.echoVecI8(new byte[]{-1, 0, 7});
        assert i8s.length == 3 : "echoVecI8 length";
        assert i8s[0] == -1 && i8s[2] == 7 : "echoVecI8 values";

        byte[] u8s = Demo.echoVecU8(new byte[]{0, 1, 2, 3});
        assert u8s.length == 4 : "echoVecU8 length";
        assert u8s[0] == 0 && u8s[3] == 3 : "echoVecU8 values";

        short[] i16s = Demo.echoVecI16(new short[]{-3, 0, 9});
        assert i16s.length == 3 : "echoVecI16 length";
        assert i16s[0] == -3 && i16s[2] == 9 : "echoVecI16 values";

        short[] u16s = Demo.echoVecU16(new short[]{0, 10, 20});
        assert u16s.length == 3 : "echoVecU16 length";
        assert u16s[0] == 0 && u16s[2] == 20 : "echoVecU16 values";

        int[] u32s = Demo.echoVecU32(new int[]{0, 10, 20});
        assert u32s.length == 3 : "echoVecU32 length";
        assert u32s[0] == 0 && u32s[2] == 20 : "echoVecU32 values";

        long[] i64s = Demo.echoVecI64(new long[]{-5L, 0L, 8L});
        assert i64s.length == 3 : "echoVecI64 length";
        assert i64s[0] == -5L && i64s[2] == 8L : "echoVecI64 values";

        long[] u64s = Demo.echoVecU64(new long[]{0L, 1L, 2L});
        assert u64s.length == 3 : "echoVecU64 length";
        assert u64s[0] == 0L && u64s[2] == 2L : "echoVecU64 values";

        long[] isizes = Demo.echoVecIsize(new long[]{-2L, 0L, 5L});
        assert isizes.length == 3 : "echoVecIsize length";
        assert isizes[0] == -2L && isizes[2] == 5L : "echoVecIsize values";

        long[] usizes = Demo.echoVecUsize(new long[]{0L, 2L, 4L});
        assert usizes.length == 3 : "echoVecUsize length";
        assert usizes[0] == 0L && usizes[2] == 4L : "echoVecUsize values";

        float[] f32s = Demo.echoVecF32(new float[]{1.25f, -2.5f});
        assert f32s.length == 2 : "echoVecF32 length";
        assert Math.abs(f32s[0] - 1.25f) < 0.0001f : "echoVecF32[0]";
        assert Math.abs(f32s[1] + 2.5f) < 0.0001f : "echoVecF32[1]";

        int[] range = Demo.makeRange(0, 5);
        assert range.length == 5 : "makeRange length";
        assert range[0] == 0 && range[4] == 4 : "makeRange values";

        int[] reversed = Demo.reverseVecI32(new int[]{1, 2, 3});
        assert reversed[0] == 3 && reversed[1] == 2 && reversed[2] == 1 : "reverseVecI32";

        System.out.println("  PASS\n");
    }

    private static void testVecStrings() {
        System.out.println("Testing vec strings...");

        List<String> strings = Demo.echoVecString(Arrays.asList("hello", "world"));
        assert strings.size() == 2 : "echoVecString size";
        assert strings.get(0).equals("hello") : "echoVecString[0]";
        assert strings.get(1).equals("world") : "echoVecString[1]";

        List<String> emptyStrings = Demo.echoVecString(Collections.emptyList());
        assert emptyStrings.isEmpty() : "echoVecString empty";

        int[] lengths = Demo.vecStringLengths(Arrays.asList("hi", "café"));
        assert lengths.length == 2 : "vecStringLengths size";
        assert lengths[0] == 2 : "vecStringLengths[0]";
        assert lengths[1] == 5 : "vecStringLengths[1] (utf8)";

        System.out.println("  PASS\n");
    }

    private static void testOptions() {
        System.out.println("Testing options...");

        Optional<Integer> optI32 = Demo.echoOptionalI32(Optional.of(7));
        assert optI32.isPresent() && optI32.get() == 7 : "echoOptionalI32 some";
        assert !Demo.echoOptionalI32(Optional.empty()).isPresent() : "echoOptionalI32 none";

        assert Demo.unwrapOrDefaultI32(Optional.of(9), 4) == 9 : "unwrapOrDefaultI32 some";
        assert Demo.unwrapOrDefaultI32(Optional.empty(), 4) == 4 : "unwrapOrDefaultI32 none";

        assert Demo.makeSomeI32(12).orElse(-1) == 12 : "makeSomeI32";
        assert !Demo.makeNoneI32().isPresent() : "makeNoneI32";

        assert Demo.doubleIfSome(Optional.of(8)).orElse(-1) == 16 : "doubleIfSome some";
        assert !Demo.doubleIfSome(Optional.empty()).isPresent() : "doubleIfSome none";

        Optional<String> optString = Demo.echoOptionalString(Optional.of("hello"));
        assert optString.isPresent() && optString.get().equals("hello") : "echoOptionalString some";
        assert !Demo.echoOptionalString(Optional.empty()).isPresent() : "echoOptionalString none";
        assert Demo.isSomeString(Optional.of("x")) : "isSomeString some";
        assert !Demo.isSomeString(Optional.empty()) : "isSomeString none";

        Optional<Point> optPoint = Demo.echoOptionalPoint(Optional.of(new Point(1.0, 2.0)));
        assert optPoint.isPresent() : "echoOptionalPoint some";
        assert optPoint.get().x() == 1.0 && optPoint.get().y() == 2.0 : "echoOptionalPoint value";
        assert Demo.makeSomePoint(3.0, 4.0).isPresent() : "makeSomePoint";
        assert !Demo.makeNonePoint().isPresent() : "makeNonePoint";

        Optional<Status> optStatus = Demo.echoOptionalStatus(Optional.of(Status.ACTIVE));
        assert optStatus.isPresent() && optStatus.get() == Status.ACTIVE : "echoOptionalStatus some";
        assert !Demo.echoOptionalStatus(Optional.empty()).isPresent() : "echoOptionalStatus none";

        Optional<int[]> optVec = Demo.echoOptionalVec(Optional.of(new int[]{1, 2, 3}));
        assert optVec.isPresent() : "echoOptionalVec some";
        assert optVec.get().length == 3 && optVec.get()[0] == 1 && optVec.get()[2] == 3 : "echoOptionalVec value";
        assert !Demo.echoOptionalVec(Optional.empty()).isPresent() : "echoOptionalVec none";

        Optional<Integer> optVecLen = Demo.optionalVecLength(Optional.of(new int[]{9, 8}));
        assert optVecLen.isPresent() && optVecLen.get() == 2 : "optionalVecLength some";
        assert !Demo.optionalVecLength(Optional.empty()).isPresent() : "optionalVecLength none";

        UserProfile withEmail = Demo.makeUserProfile(
            "Alice",
            30,
            Optional.of("alice@example.com"),
            Optional.of(98.5)
        );
        assert withEmail.email().isPresent() : "makeUserProfile email present";
        assert withEmail.score().isPresent() : "makeUserProfile score present";
        assert Demo.userDisplayName(withEmail).equals("Alice <alice@example.com>") : "userDisplayName with email";

        UserProfile noEmail = Demo.makeUserProfile(
            "Bob",
            22,
            Optional.empty(),
            Optional.empty()
        );
        assert !noEmail.email().isPresent() : "makeUserProfile email none";
        assert !noEmail.score().isPresent() : "makeUserProfile score none";
        assert Demo.userDisplayName(noEmail).equals("Bob") : "userDisplayName without email";

        UserProfile echoedProfile = Demo.echoUserProfile(withEmail);
        assert echoedProfile.email().isPresent() : "echoUserProfile email";
        assert echoedProfile.email().get().equals("alice@example.com") : "echoUserProfile email value";

        SearchResult withCursor = Demo.echoSearchResult(
            new SearchResult("rust ffi", 12, Optional.of("cursor-1"), Optional.of(0.99))
        );
        assert withCursor.nextCursor().isPresent() : "echoSearchResult cursor present";
        assert withCursor.maxScore().isPresent() : "echoSearchResult score present";
        assert Demo.hasMoreResults(withCursor) : "hasMoreResults true";

        SearchResult withoutCursor = Demo.echoSearchResult(
            new SearchResult("rust ffi", 12, Optional.empty(), Optional.empty())
        );
        assert !withoutCursor.nextCursor().isPresent() : "echoSearchResult cursor none";
        assert !withoutCursor.maxScore().isPresent() : "echoSearchResult score none";
        assert !Demo.hasMoreResults(withoutCursor) : "hasMoreResults false";

        System.out.println("  PASS\n");
    }

    private static void testRecordsWithVecs() {
        System.out.println("Testing records with vecs...");

        Polygon polygon = Demo.makePolygon(Arrays.asList(
            new Point(0.0, 0.0), new Point(1.0, 0.0), new Point(0.0, 1.0)
        ));
        assert Demo.polygonVertexCount(polygon) == 3 : "polygonVertexCount";

        Polygon echoed = Demo.echoPolygon(polygon);
        assert echoed.points().size() == 3 : "echoPolygon size";
        assert echoed.points().get(0).x() == 0.0 : "echoPolygon[0].x";

        Point centroid = Demo.polygonCentroid(polygon);
        assert Math.abs(centroid.x() - 1.0 / 3.0) < 0.0001 : "polygonCentroid.x";
        assert Math.abs(centroid.y() - 1.0 / 3.0) < 0.0001 : "polygonCentroid.y";

        Team team = Demo.makeTeam("devs", Arrays.asList("Alice", "Bob"));
        assert team.name().equals("devs") : "makeTeam.name";
        assert team.members().size() == 2 : "makeTeam.members.size";

        Team echoedTeam = Demo.echoTeam(team);
        assert echoedTeam.members().get(0).equals("Alice") : "echoTeam.members[0]";
        assert Demo.teamSize(team) == 2 : "teamSize";

        Classroom classroom = Demo.makeClassroom(Arrays.asList(
            new Person("Mia", 10),
            new Person("Leo", 11)
        ));
        assert classroom.students().size() == 2 : "makeClassroom.students.size";
        assert classroom.students().get(0).name().equals("Mia") : "makeClassroom.students[0].name";

        Classroom echoedClassroom = Demo.echoClassroom(classroom);
        assert echoedClassroom.students().size() == 2 : "echoClassroom.students.size";
        assert echoedClassroom.students().get(1).name().equals("Leo") : "echoClassroom.students[1].name";

        TaggedScores ts = Demo.echoTaggedScores(new TaggedScores("math", new double[]{90.0, 85.5}));
        assert ts.label().equals("math") : "echoTaggedScores.label";
        assert ts.scores().length == 2 : "echoTaggedScores.scores.length";
        assert Math.abs(Demo.averageScore(new TaggedScores("x", new double[]{80.0, 100.0})) - 90.0) < 0.0001 : "averageScore";

        System.out.println("  PASS\n");
    }
}
