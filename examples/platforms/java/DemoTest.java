package com.boltffi.demo;

import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Optional;
import java.util.concurrent.CompletableFuture;

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
        testClosures();
        testSyncCallbacks();
        testAsyncCallbacks();
        testAsyncFunctions();
        testAsyncClassMethods();
        testResultFunctions();
        testResultClassMethods();
        testResultEnumErrors();
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
        assert Math.abs(MathUtils.distanceBetween(new Point(0.0, 0.0), new Point(3.0, 4.0)) - 5.0) < 0.0001 : "MathUtils.distanceBetween";
        Point midpoint = MathUtils.midpoint(new Point(0.0, 0.0), new Point(2.0, 4.0));
        assert midpoint.x() == 1.0 : "MathUtils.midpoint.x";
        assert midpoint.y() == 2.0 : "MathUtils.midpoint.y";
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

        Shape rect = Demo.makeRectangle(3.0, 4.0);
        assert rect instanceof Shape.Rectangle : "makeRectangle returns Rectangle";

        Shape echoedCircle = Demo.echoShape(circle);
        assert echoedCircle instanceof Shape.Circle : "echoShape(circle) type";
        assert ((Shape.Circle) echoedCircle).radius == 5.0 : "echoShape(circle).radius";

        Shape echoedRect = Demo.echoShape(rect);
        assert echoedRect instanceof Shape.Rectangle : "echoShape(rect) type";

        Shape triangle = Demo.echoShape(new Shape.Triangle(
            new Point(0.0, 0.0), new Point(3.0, 0.0), new Point(0.0, 4.0)
        ));
        assert triangle instanceof Shape.Triangle : "echoShape(triangle) type";

        Shape point = Demo.echoShape(Shape.Point.INSTANCE);
        assert point instanceof Shape.Point : "echoShape(point) type";

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

    private static void testClosures() {
        System.out.println("Testing closures...");

        final int[] observedValue = new int[]{0};

        assert Demo.applyClosure(value -> value * 2, 5) == 10 : "applyClosure";
        Demo.applyVoidClosure(value -> observedValue[0] = value, 42);
        assert observedValue[0] == 42 : "applyVoidClosure";
        assert Demo.applyNullaryClosure(() -> 99) == 99 : "applyNullaryClosure";
        assert Demo.applyStringClosure(String::toUpperCase, "hello").equals("HELLO") : "applyStringClosure";
        assert !Demo.applyBoolClosure(value -> !value, true) : "applyBoolClosure";
        assert Math.abs(Demo.applyF64Closure(value -> value * value, 3.0) - 9.0) < 0.0001 : "applyF64Closure";
        assert Demo.applyBinaryClosure((left, right) -> left + right, 3, 4) == 7 : "applyBinaryClosure";

        Point transformedPoint = Demo.applyPointClosure(
            point -> new Point(point.x() + 1.0, point.y() + 1.0),
            new Point(1.0, 2.0)
        );
        assert transformedPoint.x() == 2.0 : "applyPointClosure.x";
        assert transformedPoint.y() == 3.0 : "applyPointClosure.y";

        int[] mapped = Demo.mapVecWithClosure(value -> value * 2, new int[]{1, 2, 3});
        assert mapped.length == 3 : "mapVecWithClosure length";
        assert mapped[0] == 2 && mapped[1] == 4 && mapped[2] == 6 : "mapVecWithClosure values";

        int[] filtered = Demo.filterVecWithClosure(value -> value % 2 == 0, new int[]{1, 2, 3, 4});
        assert filtered.length == 2 : "filterVecWithClosure length";
        assert filtered[0] == 2 && filtered[1] == 4 : "filterVecWithClosure values";

        System.out.println("  PASS\n");
    }

    private static void testSyncCallbacks() {
        System.out.println("Testing sync callbacks...");

        ValueCallback doubler = value -> value * 2;
        ValueCallback tripler = value -> value * 3;
        PointTransformer pointTransformer = point -> new Point(point.x() + 10.0, point.y() + 20.0);
        StatusMapper statusMapper = status -> status == Status.PENDING ? Status.ACTIVE : Status.INACTIVE;
        MultiMethodCallback multiMethod = new MultiMethodCallback() {
            @Override
            public int methodA(int x) {
                return x + 1;
            }

            @Override
            public int methodB(int x, int y) {
                return x * y;
            }

            @Override
            public int methodC() {
                return 5;
            }
        };
        OptionCallback optionCallback = key -> key > 0 ? Optional.of(key * 10) : Optional.empty();
        VecProcessor vecProcessor = values -> Arrays.stream(values).map(value -> value * value).toArray();

        assert Demo.invokeValueCallback(doubler, 4) == 8 : "invokeValueCallback";
        assert Demo.invokeValueCallbackTwice(doubler, 3, 4) == 14 : "invokeValueCallbackTwice";
        assert Demo.invokeBoxedValueCallback(doubler, 5) == 10 : "invokeBoxedValueCallback";
        assert Demo.mapStatus(statusMapper, Status.PENDING) == Status.ACTIVE : "mapStatus";

        int[] processed = Demo.processVec(vecProcessor, new int[]{1, 2, 3});
        assert processed.length == 3 : "processVec length";
        assert processed[0] == 1 && processed[1] == 4 && processed[2] == 9 : "processVec values";

        assert Demo.invokeMultiMethod(multiMethod, 3, 4) == 21 : "invokeMultiMethod";
        assert Demo.invokeMultiMethodBoxed(multiMethod, 3, 4) == 21 : "invokeMultiMethodBoxed";
        assert Demo.invokeTwoCallbacks(doubler, tripler, 5) == 25 : "invokeTwoCallbacks";

        Optional<Integer> optionResult = Demo.invokeOptionCallback(optionCallback, 7);
        assert optionResult.isPresent() && optionResult.get() == 70 : "invokeOptionCallback some";
        assert !Demo.invokeOptionCallback(optionCallback, 0).isPresent() : "invokeOptionCallback none";

        Point transformed = Demo.transformPoint(pointTransformer, new Point(1.0, 2.0));
        assert transformed.x() == 11.0 : "transformPoint.x";
        assert transformed.y() == 22.0 : "transformPoint.y";

        Point transformedBoxed = Demo.transformPointBoxed(pointTransformer, new Point(3.0, 4.0));
        assert transformedBoxed.x() == 13.0 : "transformPointBoxed.x";
        assert transformedBoxed.y() == 24.0 : "transformPointBoxed.y";

        System.out.println("  PASS\n");
    }

    private static void testAsyncCallbacks() {
        System.out.println("Testing async callbacks...");
        try {
            AsyncFetcher asyncFetcher = new AsyncFetcher() {
                @Override
                public CompletableFuture<Integer> fetchValue(int key) {
                    return CompletableFuture.completedFuture(key * 100);
                }

                @Override
                public CompletableFuture<String> fetchString(String input) {
                    return CompletableFuture.completedFuture(input.toUpperCase());
                }
            };
            AsyncOptionFetcher asyncOptionFetcher = key -> CompletableFuture.completedFuture(
                key > 0 ? Optional.of(key * 1000L) : Optional.empty()
            );

            assert Demo.fetchWithAsyncCallback(asyncFetcher, 5).get() == 500 : "fetchWithAsyncCallback";
            assert Demo.fetchStringWithAsyncCallback(asyncFetcher, "boltffi").get().equals("BOLTFFI") : "fetchStringWithAsyncCallback";

            Optional<Long> some = Demo.invokeAsyncOptionFetcher(asyncOptionFetcher, 7).get();
            assert some.isPresent() && some.get() == 7000L : "invokeAsyncOptionFetcher some";

            Optional<Long> none = Demo.invokeAsyncOptionFetcher(asyncOptionFetcher, 0).get();
            assert !none.isPresent() : "invokeAsyncOptionFetcher none";
        } catch (Exception exception) {
            throw new RuntimeException("async callback test failed", exception);
        }
        System.out.println("  PASS\n");
    }

    private static void testAsyncFunctions() {
        System.out.println("Testing async functions...");
        try {
            CompletableFuture<Integer> addFuture = Demo.asyncAdd(3, 7);
            assert addFuture.get() == 10 : "asyncAdd(3, 7)";

            CompletableFuture<String> echoFuture = Demo.asyncEcho("hello async");
            assert echoFuture.get().equals("Echo: hello async") : "asyncEcho";

            CompletableFuture<int[]> doubleFuture = Demo.asyncDoubleAll(new int[]{1, 2, 3});
            int[] doubled = doubleFuture.get();
            assert doubled.length == 3 : "asyncDoubleAll length";
            assert doubled[0] == 2 && doubled[1] == 4 && doubled[2] == 6 : "asyncDoubleAll values";

            CompletableFuture<Optional<Integer>> findSome = Demo.asyncFindPositive(new int[]{-1, 0, 5, 3});
            assert findSome.get().isPresent() && findSome.get().get() == 5 : "asyncFindPositive some";

            CompletableFuture<Optional<Integer>> findNone = Demo.asyncFindPositive(new int[]{-1, -2, -3});
            assert !findNone.get().isPresent() : "asyncFindPositive none";

            CompletableFuture<String> concatFuture = Demo.asyncConcat(Arrays.asList("a", "b", "c"));
            assert concatFuture.get().equals("a, b, c") : "asyncConcat";
        } catch (Exception e) {
            throw new RuntimeException("async function test failed", e);
        }
        System.out.println("  PASS\n");
    }

    private static void testAsyncClassMethods() {
        System.out.println("Testing async class methods...");
        try {
            AsyncWorker worker = new AsyncWorker("test");
            assert worker.getPrefix().equals("test") : "AsyncWorker.getPrefix";

            String processed = worker.process("data").get();
            assert processed.equals("test: data") : "AsyncWorker.process";

            Optional<String> found = worker.findItem(42).get();
            assert found.isPresent() : "AsyncWorker.findItem some";
            assert found.get().equals("test_42") : "AsyncWorker.findItem value";

            Optional<String> notFound = worker.findItem(-1).get();
            assert !notFound.isPresent() : "AsyncWorker.findItem none";

            List<String> batch = worker.processBatch(Arrays.asList("x", "y")).get();
            assert batch.size() == 2 : "AsyncWorker.processBatch size";
            assert batch.get(0).equals("test: x") : "AsyncWorker.processBatch[0]";
            assert batch.get(1).equals("test: y") : "AsyncWorker.processBatch[1]";

            worker.close();
        } catch (Exception e) {
            throw new RuntimeException("async class method test failed", e);
        }
        System.out.println("  PASS\n");
    }

    private static void testResultFunctions() {
        System.out.println("Testing result functions...");

        assert Demo.safeDivide(10, 2) == 5 : "safeDivide ok";
        try {
            Demo.safeDivide(10, 0);
            assert false : "safeDivide should throw on zero divisor";
        } catch (RuntimeException e) {
            assert e.getMessage().contains("division by zero") : "safeDivide error message";
        }

        assert Demo.alwaysOk(21) == 42 : "alwaysOk";
        try {
            Demo.alwaysErr("boom");
            assert false : "alwaysErr should throw";
        } catch (RuntimeException e) {
            assert e.getMessage().contains("boom") : "alwaysErr error message";
        }

        Point p = Demo.parsePoint("3.0,4.0");
        assert p.x() == 3.0 : "parsePoint x";
        assert p.y() == 4.0 : "parsePoint y";
        try {
            Demo.parsePoint("bad");
            assert false : "parsePoint should throw on bad input";
        } catch (RuntimeException ignored) {}

        assert Demo.resultOfString(1).equals("item_1") : "resultOfString ok";
        try {
            Demo.resultOfString(-1);
            assert false : "resultOfString should throw on negative key";
        } catch (RuntimeException ignored) {}

        Optional<Integer> some = Demo.resultOfOption(5);
        assert some.isPresent() && some.get() == 10 : "resultOfOption present";
        Optional<Integer> none = Demo.resultOfOption(0);
        assert !none.isPresent() : "resultOfOption empty";
        try {
            Demo.resultOfOption(-1);
            assert false : "resultOfOption should throw on negative key";
        } catch (RuntimeException ignored) {}

        int[] vec = Demo.resultOfVec(3);
        assert vec.length == 3 : "resultOfVec length";
        assert vec[0] == 0 && vec[1] == 1 && vec[2] == 2 : "resultOfVec values";
        try {
            Demo.resultOfVec(-1);
            assert false : "resultOfVec should throw on negative count";
        } catch (RuntimeException ignored) {}

        System.out.println("  PASS\n");
    }

    private static void testResultClassMethods() {
        System.out.println("Testing result class methods...");

        try (Counter counter = new Counter(0)) {
            counter.increment();
            counter.increment();
            counter.increment();
            int val = counter.tryGetPositive();
            assert val == 3 : "tryGetPositive ok: " + val;
        }

        try (Counter counter = new Counter(0)) {
            try {
                counter.tryGetPositive();
                assert false : "tryGetPositive should throw when zero";
            } catch (RuntimeException ignored) {}
        }

        System.out.println("  PASS\n");
    }

    private static void testResultEnumErrors() {
        System.out.println("Testing result enum errors...");

        assert Demo.checkedDivide(10, 2) == 5 : "checkedDivide ok";
        try {
            Demo.checkedDivide(10, 0);
            assert false : "checkedDivide should throw on zero divisor";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.DIVISION_BY_ZERO : "checkedDivide typed error";
        }

        assert Demo.checkedSqrt(9.0) == 3.0 : "checkedSqrt ok";
        try {
            Demo.checkedSqrt(-1.0);
            assert false : "checkedSqrt should throw on negative";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.NEGATIVE_INPUT : "checkedSqrt typed error";
        }

        assert Demo.checkedAdd(1, 2) == 3 : "checkedAdd ok";
        try {
            Demo.checkedAdd(Integer.MAX_VALUE, 1);
            assert false : "checkedAdd should throw on overflow";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.OVERFLOW : "checkedAdd typed error";
        }

        assert Demo.validateUsername("alice").equals("alice") : "validateUsername ok";
        try {
            Demo.validateUsername("ab");
            assert false : "validateUsername should throw on short name";
        } catch (ValidationError.Exception e) {
            assert e.getError() == ValidationError.TOO_SHORT : "validateUsername typed error";
        }
        try {
            Demo.validateUsername("a]bcdefghijklmnopqrstu");
            assert false : "validateUsername should throw on long name";
        } catch (ValidationError.Exception e) {
            assert e.getError() == ValidationError.TOO_LONG : "validateUsername typed error";
        }
        try {
            Demo.validateUsername("has space");
            assert false : "validateUsername should throw on spaces";
        } catch (ValidationError.Exception e) {
            assert e.getError() == ValidationError.INVALID_FORMAT : "validateUsername typed error";
        }

        System.out.println("  PASS\n");
    }
}
