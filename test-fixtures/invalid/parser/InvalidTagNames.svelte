<!-- Parser error: InvalidTagName - Tag names with invalid characters -->

<!-- Tags starting with numbers -->
<1div>Invalid start</1div>
<123>All numbers</123>

<!-- Tags with spaces -->
<div class>Space in tag name</div>

<!-- Tags with special characters -->
<div@class>At symbol</div@class>
<div#id>Hash symbol</div#id>
<div.class>Dot in tag</div.class>
<div+plus>Plus symbol</div+plus>
<div=equals>Equals symbol</div=equals>

<!-- Empty tag name -->
<>Empty tag</>

<!-- Tags with only hyphens or underscores that are invalid -->
<-->Just hyphens</-->
<__>Just underscores</__>

<!-- Invalid svelte: prefix usage -->
<svelte:>Empty svelte tag</svelte:>
<svelte:invalid>Unknown svelte element</svelte:invalid>
<svelte:123>Numeric svelte element</svelte:123>

<!-- Invalid closing tag characters -->
<div>Content</div >
<div>Content</ div>
<div>Content</d iv>

<!-- XML-style namespaced tags (not valid in Svelte HTML context) -->
<xml:element>Content</xml:element>
<foo:bar>Namespaced</foo:bar>

<!-- Tags that look like expressions -->
<{expression}>Dynamic tag syntax error</{expression}>
