# Operating-mode artwork

Generated with the built-in imagegen tool for the resource-style choice tiles. Source files live
in `assets/images/resources/`; the normal asset pipeline produces runtime textures.

The common prompt was:

> Create ONE finished UI button image for the Stellarion space strategy game. Use case:
> stylized-concept. Landscape 3:2 image. Dark navy-blue subtly textured metal background with faint
> diagonal scratches, cyan-blue glow, matching a science-fiction game control tile. Center
> [subject]. Very clean recognizable silhouette, large central motif occupying about half the
> frame, generous empty dark margin. Subtle beveled illuminated glass-metal look, restrained bloom.
> No text, numbers, labels, border, watermark, UI mockup, or extra symbols. Save as a single image,
> not a contact sheet.

| File | Subject |
| --- | --- |
| `mine normal.png` | One bold luminous cyan right-pointing play triangle, meaning normal production |
| `mine intensive.png` | Two bold luminous cyan upward chevrons with a small lightning bolt, meaning intensive production |
| `mine suspended.png` | Two bold luminous cyan vertical pause bars, meaning suspended production |
| `dock industrial.png` | One bold luminous cyan industrial cog with a simplified spacecraft silhouette in its center, meaning industrial space dock |
| `dock bastion.png` | One bold luminous cyan heavy shield emblem with a small star in its center, meaning defensive bastion space dock |

Recycler choices reuse the existing no-focus and resource artwork.

Senate policies use two additional tiles generated with the same prompt (omitting the
`Use case: stylized-concept.` sentence):

| File | Subject |
| --- | --- |
| `senate expansion.png` | Three bold luminous cyan spacecraft silhouettes advancing together in a forward-pointing formation, meaning expansion and ship construction |
| `senate consolidation.png` | One bold luminous cyan planetary fortress encircled by a protective shield, meaning consolidation and defense construction |
