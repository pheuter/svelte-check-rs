<!-- Upstream parity: parser-legacy/implicitly-closed-li + implicitly-closed-li-block -->
<ul>
    <li>one
    <li>two
    <li>three
</ul>

<ul>
    {#each items as item}
        <li>{item}
    {/each}
</ul>

<ol>
    <li>alpha
    <li>beta
</ol>
