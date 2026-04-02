import { run as runContract } from "./tests/contract.test.mjs";
import { run as runAsyncFns } from "./tests/async_fns/mod.test.mjs";
import { run as runBuiltins } from "./tests/builtins/mod.test.mjs";
import { run as runBytes } from "./tests/bytes/mod.test.mjs";
import { run as runAsyncTraits } from "./tests/callbacks/async_traits.test.mjs";
import { run as runClosures } from "./tests/callbacks/closures.test.mjs";
import { run as runSyncTraits } from "./tests/callbacks/sync_traits.test.mjs";
import { run as runAsyncMethods } from "./tests/classes/async_methods.test.mjs";
import { run as runConstructorMatrix } from "./tests/classes/constructor_matrix.test.mjs";
import { run as runConstructors } from "./tests/classes/constructors.test.mjs";
import { run as runMethods } from "./tests/classes/methods.test.mjs";
import { run as runStaticMethods } from "./tests/classes/static_methods.test.mjs";
import { run as runStreams } from "./tests/classes/streams.test.mjs";
import { run as runThreadSafe } from "./tests/classes/thread_safe.test.mjs";
import { run as runUnsafeSingleThreaded } from "./tests/classes/unsafe_single_threaded.test.mjs";
import { run as runCustomTypes } from "./tests/custom_types/mod.test.mjs";
import { run as runCStyleEnums } from "./tests/enums/c_style.test.mjs";
import { run as runComplexVariants } from "./tests/enums/complex_variants.test.mjs";
import { run as runDataEnums } from "./tests/enums/data_enum.test.mjs";
import { run as runReprIntEnums } from "./tests/enums/repr_int.test.mjs";
import { run as runComplexOptions } from "./tests/options/complex.test.mjs";
import { run as runPrimitiveOptions } from "./tests/options/primitives.test.mjs";
import { run as runScalars } from "./tests/primitives/scalars.test.mjs";
import { run as runStrings } from "./tests/primitives/strings.test.mjs";
import { run as runVecs } from "./tests/primitives/vecs.test.mjs";
import { run as runBlittableRecords } from "./tests/records/blittable.test.mjs";
import { run as runDefaultValueRecords } from "./tests/records/default_values.test.mjs";
import { run as runNestedRecords } from "./tests/records/nested.test.mjs";
import { run as runCollectionRecords } from "./tests/records/with_collections.test.mjs";
import { run as runEnumRecords } from "./tests/records/with_enums.test.mjs";
import { run as runOptionRecords } from "./tests/records/with_options.test.mjs";
import { run as runStringRecords } from "./tests/records/with_strings.test.mjs";
import { run as runAsyncResults } from "./tests/results/async_results.test.mjs";
import { run as runBasicResults } from "./tests/results/basic.test.mjs";
import { run as runErrorEnumResults } from "./tests/results/error_enums.test.mjs";
import { run as runErrorStructResults } from "./tests/results/error_structs.test.mjs";
import { run as runNestedResults } from "./tests/results/nested_results.test.mjs";

const suites = [
  runContract,
  runAsyncFns,
  runBuiltins,
  runBytes,
  runAsyncTraits,
  runClosures,
  runSyncTraits,
  runAsyncMethods,
  runConstructorMatrix,
  runConstructors,
  runMethods,
  runStaticMethods,
  runStreams,
  runThreadSafe,
  runUnsafeSingleThreaded,
  runCustomTypes,
  runCStyleEnums,
  runComplexVariants,
  runDataEnums,
  runReprIntEnums,
  runComplexOptions,
  runPrimitiveOptions,
  runScalars,
  runStrings,
  runVecs,
  runBlittableRecords,
  runDefaultValueRecords,
  runNestedRecords,
  runCollectionRecords,
  runEnumRecords,
  runOptionRecords,
  runStringRecords,
  runAsyncResults,
  runBasicResults,
  runErrorEnumResults,
  runErrorStructResults,
  runNestedResults,
];

await suites.reduce(
  (previousSuite, suite) => previousSuite.then(() => suite()),
  Promise.resolve(),
);

console.log("\nAll wasm tests passed!");
