<script lang="ts">
    import type { Attachment } from 'svelte/attachments';

    // Simple attachment function
    const myAttachment: Attachment = (element) => {
        console.log(element.nodeName);
        return () => console.log('cleanup');
    };

    // Attachment factory pattern
    function tooltip(content: string): Attachment {
        return (element) => {
            console.log('Setting tooltip:', content);
            return () => console.log('Removing tooltip');
        };
    }

    // State for reactive attachments
    let color = $state('red');
    let tooltipText = $state('Hello!');
</script>

<!-- Simple attachment on element -->
<div {@attach myAttachment}>Simple attach</div>

<!-- Inline arrow function attachment -->
<canvas
    width={32}
    height={32}
    {@attach (canvas) => {
        const context = canvas.getContext('2d');
        if (context) {
            context.fillStyle = color;
            context.fillRect(0, 0, canvas.width, canvas.height);
        }
    }}
></canvas>

<!-- Attachment factory with reactive content -->
<button {@attach tooltip(tooltipText)}>
    Hover me
</button>

<!-- Multiple attachments on single element -->
<div {@attach myAttachment} {@attach tooltip('Multiple')}>
    Multiple attachments
</div>

<!-- Attachment with other attributes -->
<div class="container" {@attach myAttachment} id="main">
    Mixed attributes
</div>

<!-- Self-closing element with attachment -->
<input type="text" {@attach myAttachment} />
