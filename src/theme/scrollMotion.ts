/**
 * Move only the results viewport, not the entire Tauri window.
 * With animation disabled, assigning scrollTop guarantees an immediate jump
 * even if a future CSS rule adds scroll-behavior: smooth.
 */
export function scrollResultsToTop(
  viewport: {scrollTop:number;scrollTo(options:ScrollToOptions):void}|null|undefined,
  animationEnabled:boolean,
  reducedMotion:boolean,
):void {
  if(!viewport)return;
  if(animationEnabled&&!reducedMotion) {
    viewport.scrollTo({top:0,behavior:'smooth'});
  }else{
    viewport.scrollTop=0;
  }
}
