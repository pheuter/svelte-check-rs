<script lang="ts">
    import type { Action } from 'svelte/action';

    // Valid action function
    function tooltip(node: HTMLElement, content?: string) {
        node.title = content ?? 'Default tooltip';
        return {
            update(newContent: string) {
                node.title = newContent;
            },
            destroy() {
                node.title = '';
            }
        };
    }

    // Valid action object with member access
    const formActions = {
        enhance: ((node: HTMLFormElement) => {
            return {
                destroy() {}
            };
        }) as Action<HTMLFormElement>,
        validate: ((node: HTMLFormElement, options: { required: boolean }) => {
            return {
                destroy() {}
            };
        }) as Action<HTMLFormElement, { required: boolean }>
    };

    // Nested action object
    const ui = {
        actions: {
            draggable: ((node: HTMLElement) => {
                return { destroy() {} };
            }) as Action<HTMLElement>,
            resizable: ((node: HTMLElement, options: { minWidth: number }) => {
                return { destroy() {} };
            }) as Action<HTMLElement, { minWidth: number }>
        }
    };

    // Variables for testing
    let tooltipContent = 'Hello world';
    let validateOptions = { required: true };
    let resizeOptions = { minWidth: 100 };

    // ERROR CASES: These should produce type errors

    // This object does NOT have an 'enhance' property
    const invalidActions = {
        submit: (node: HTMLFormElement) => ({ destroy() {} })
    };

    // Wrong type for action parameter
    let wrongType = "not an object";
</script>

<!-- VALID USE CASES - these should NOT produce errors -->

<!-- Basic use directive -->
<div use:tooltip>Basic tooltip</div>

<!-- Use directive with parameter -->
<div use:tooltip={tooltipContent}>Tooltip with content</div>

<!-- Use directive with member access (Issue #7 main case) -->
<form method="POST" use:formActions.enhance>
    <button type="submit">Submit with enhance</button>
</form>

<!-- Use directive with member access and parameter -->
<form method="POST" use:formActions.validate={validateOptions}>
    <button type="submit">Submit with validation</button>
</form>

<!-- Deep member access -->
<div use:ui.actions.draggable>Draggable element</div>

<!-- Deep member access with parameter -->
<div use:ui.actions.resizable={resizeOptions}>Resizable element</div>

<!-- Multiple use directives including member access -->
<div use:tooltip use:ui.actions.draggable>Multiple directives</div>

<!-- ERROR CASES - these SHOULD produce errors on the correct lines -->

<!-- Line 89: Error - 'enhance' does not exist on 'invalidActions' -->
<form use:invalidActions.enhance>
    <button>This should error</button>
</form>

<!-- Line 94: Error - wrong parameter type -->
<form use:formActions.validate={wrongType}>
    <button>Wrong type parameter</button>
</form>

<!-- Line 99: Error - nonexistent nested property -->
<div use:ui.actions.nonExistent>
    Nonexistent action
</div>
