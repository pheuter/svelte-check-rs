<script lang="ts">
    type Relationship = {
        relationshipType: string;
        shortName?: string | undefined;
    };
</script>

{#snippet relationshipFormFields<
    T extends Relationship,
>(args: T)}
    <div>{args.relationshipType}</div>
{/snippet}

{@render relationshipFormFields({ relationshipType: "friend" })}
