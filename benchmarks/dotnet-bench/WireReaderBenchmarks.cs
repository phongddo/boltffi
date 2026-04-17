using BenchmarkDotNet.Attributes;
using BenchBoltffi;

namespace BoltFFIBench;

// Each case is chosen to isolate one axis of the WireReader design:
//   - PrimitiveReturn_*   : no FfiBuf, no WireReader. Noise floor for P/Invoke.
//   - EchoString_Small    : FfiBuf + WireReader dominated by fixed per-call cost.
//   - EchoString_Large    : FfiBuf + WireReader dominated by the eager full-buffer copy.
//   - GenerateString_*    : return-only string decode, no param write.
//   - CreateBoundingBox   : primitive-only nested record (4 × f64) — isolates
//                           managed-array-indexed primitive reads vs any
//                           pointer-backed alternative.
//   - CreateAddress       : string-bearing record with two short strings.
//   - CreatePerson        : nested string-bearing record (param write + return decode).
[MemoryDiagnoser]
public class WireReaderBenchmarks
{
    private string _smallString = null!;
    private string _largeString = null!;
    private Address _address;

    [GlobalSetup]
    public void Setup()
    {
        _smallString = "hello world";
        _largeString = new string('x', 64 * 1024);
        _address = new Address("123 Main St", "Seattle", 98101);
    }

    // --- Primitive controls (no WireReader) ---

    [Benchmark]
    public void PrimitiveReturn_Noop() => BenchBoltffi.BenchBoltffi.Noop();

    [Benchmark]
    public int PrimitiveReturn_EchoI32() => BenchBoltffi.BenchBoltffi.EchoI32(42);

    [Benchmark]
    public int PrimitiveReturn_Add() => BenchBoltffi.BenchBoltffi.Add(17, 25);

    // --- String decode via WireReader ---

    [Benchmark]
    public string EchoString_Small() => BenchBoltffi.BenchBoltffi.EchoString(_smallString);

    [Benchmark]
    public string EchoString_Large() => BenchBoltffi.BenchBoltffi.EchoString(_largeString);

    [Benchmark]
    public string GenerateString_1KB() => BenchBoltffi.BenchBoltffi.GenerateString(1024);

    [Benchmark]
    public string GenerateString_64KB() => BenchBoltffi.BenchBoltffi.GenerateString(64 * 1024);

    // --- Record decode via WireReader ---

    [Benchmark]
    public BoundingBox CreateBoundingBox_PrimitivesOnly()
        => BenchBoltffi.BenchBoltffi.CreateBoundingBox(1.0, 2.0, 3.0, 4.0);

    [Benchmark]
    public Address CreateAddress_ShortStrings()
        => BenchBoltffi.BenchBoltffi.CreateAddress("123 Main St", "Seattle", 98101);

    [Benchmark]
    public Person CreatePerson_NestedStringRecord()
        => BenchBoltffi.BenchBoltffi.CreatePerson("Alice", 30, _address);
}
