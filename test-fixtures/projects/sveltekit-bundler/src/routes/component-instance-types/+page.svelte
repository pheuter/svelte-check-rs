<script lang="ts">
    import type DirectTarget from "./components/Target.svelte";
    import type LegacyTarget from "./components/LegacyTarget.svelte";
    import JavaScriptTarget from "./components/JavaScriptTarget.svelte";
    import { GenericTarget, ScriptlessTarget, Target } from "./index";

    type Equal<Left, Right> =
        (<Value>() => Value extends Left ? 1 : 2) extends
        (<Value>() => Value extends Right ? 1 : 2)
            ? true
            : false;
    type Assert<Condition extends true> = Condition;
    type IsAny<Value> = 0 extends 1 & Value ? true : false;

    interface Item {
        id: string;
        count: number;
    }

    const item: Item = { id: "one", count: 1 };

    let target = $state<Target>();
    let genericTarget = $state<GenericTarget<Item>>();
    let javaScriptTarget = $state<JavaScriptTarget>();
    let scriptlessTarget = $state<ScriptlessTarget>();

    type _BarrelPreservesInstanceType = Assert<Equal<IsAny<Target>, false>>;
    type _DirectTypeImportMatchesBarrel = Assert<Equal<DirectTarget, Target>>;
    type _MethodReturnIsPrecise = Assert<Equal<ReturnType<Target["readLabel"]>, string>>;
    type _LegacySetPropsArePrecise = Assert<
        Equal<Parameters<NonNullable<Target["$set"]>>[0], { label?: string }>
    >;
    type _LegacyOnUnsubscribes = Assert<
        Equal<ReturnType<NonNullable<Target["$on"]>>, () => void>
    >;
    type _GenericReturnIsPrecise = Assert<
        Equal<ReturnType<GenericTarget<Item>["current"]>, Item>
    >;
    type _GenericSetPropsArePrecise = Assert<
        Equal<Parameters<NonNullable<GenericTarget<Item>["$set"]>>[0], { item?: Item }>
    >;
    type _GenericDefaultIsPreserved = Assert<
        Equal<ReturnType<GenericTarget["current"]>, Item>
    >;
    type _JavaScriptReturnIsPrecise = Assert<
        Equal<ReturnType<JavaScriptTarget["reset"]>, number>
    >;
    type _ScriptlessInstanceIsNotAny = Assert<Equal<IsAny<ScriptlessTarget>, false>>;
    type _LegacyInstanceIsNotAny = Assert<Equal<IsAny<LegacyTarget>, false>>;

    // @ts-expect-error generic constraints must survive the merged type export
    type _InvalidGenericArgument = GenericTarget<{ name: string }>;

    // @ts-expect-error only actual component exports belong to the instance type
    type _MissingMethod = Target["missing"];

</script>

<Target label="ready" bind:this={target} />
<GenericTarget {item} bind:this={genericTarget} />
<JavaScriptTarget bind:this={javaScriptTarget} />
<ScriptlessTarget bind:this={scriptlessTarget} />

<button onclick={() => target?.readLabel()}>Read</button>
<button onclick={() => genericTarget?.current()}>Current</button>
<button onclick={() => javaScriptTarget?.reset()}>Reset</button>
