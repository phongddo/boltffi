using BenchmarkDotNet.Attributes;
using Demo;

namespace BoltFFIBench;

// Isolates the two enum marshaling paths the C# backend produces:
//   - EchoDirection_*    : C-style enum. The CLR marshals the enum as
//                          its backing int, so this should price in
//                          near the PrimitiveReturn_* noise floor from
//                          WireReaderBenchmarks.
//   - EchoTaskStatus_*   : data enum. Each call wire-encodes the
//                          variant (tag + payload) on the way in,
//                          allocates an FfiBuf on the way out, and
//                          wire-decodes on return. Measures the
//                          overhead of variable-payload wire codec
//                          against the direct baseline.
//
// The TaskStatus cases cover three payload sizes so the reader can see
// how wire cost scales with variant body: Pending (unit, tag only),
// InProgress (tag + one i32), Completed (same). Adding a large-payload
// variant would exercise buffer growth — defer until fixtures include
// one.
[MemoryDiagnoser]
public class EnumWireBenchmarks
{
    private TaskStatus _pending = null!;
    private TaskStatus _inProgress = null!;
    private TaskStatus _completed = null!;

    [GlobalSetup]
    public void Setup()
    {
        _pending = new TaskStatus.Pending();
        _inProgress = new TaskStatus.InProgress(42);
        _completed = new TaskStatus.Completed(100);
    }

    // --- C-style enum (direct P/Invoke, no wire) ---

    [Benchmark]
    public Direction EchoDirection_North()
        => Demo.Demo.EchoDirection(Direction.North);

    [Benchmark]
    public Direction EchoDirection_West()
        => Demo.Demo.EchoDirection(Direction.West);

    // --- Data enum (wire encode + decode) ---

    [Benchmark]
    public TaskStatus EchoTaskStatus_UnitVariant()
        => Demo.Demo.EchoTaskStatus(_pending);

    [Benchmark]
    public TaskStatus EchoTaskStatus_SmallPayload()
        => Demo.Demo.EchoTaskStatus(_inProgress);

    [Benchmark]
    public TaskStatus EchoTaskStatus_CompletedPayload()
        => Demo.Demo.EchoTaskStatus(_completed);
}
