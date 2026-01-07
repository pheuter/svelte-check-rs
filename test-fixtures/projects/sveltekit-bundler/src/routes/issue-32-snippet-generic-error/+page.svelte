{#snippet relationshipFormFields<
    T extends { relationshipType: string; shortName?: string | undefined },
>(args: T)}
    <div>{args.relationshipType}</div>
{/snippet}

{@render relationshipFormFields({ relationshipType: 123 })}
