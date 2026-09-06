## 1. Shared Inline Search Presentation

- [ ] 1.1 Replace the three-row bordered input arrangement with a shared one-row search presentation that accepts the destination's canonical pill and result rectangles; update the existing Inline Search renderer test to prove the bar occupies one row, no border is painted, and published result geometry equals the supplied content rectangle.

## 2. Destination Composition

- [ ] 2.1 Update Browser Normal and Wide composition to pass the existing pill slot and normal content rectangle to Inline Search instead of painting pill controls or a separate search bar; extend the existing Browser Inline Search test only as needed to verify search results replace the ordinary pill presentation.
- [ ] 2.2 Update MusicWorkspace Normal and Wide composition to use the same shared search presentation instead of group pills or a pre-painted search bar; update the existing Music workspace search characterization only as needed to verify flat search results replace the group-pill presentation.
- [ ] 2.3 Update TvWorkspace Wide composition to use the shared search presentation in the existing browser rail; update the existing TV workspace search characterization only as needed to verify search results replace the ordinary selector presentation while existing Normal/Wide transfer coverage continues to pass.
