import {test} from 'node:test';
import assert from 'node:assert/strict';
import {scrollResultsToTop} from '../src/theme/scrollMotion.ts';

function viewport(start=960){
  const moves:ScrollToOptions[]=[];
  const host={
    scrollTop:start,
    scrollTo(options:ScrollToOptions){moves.push(options);},
  };
  return {host,moves};
}

test('pagination animates the results viewport when interface motion is enabled',()=>{
  const {host,moves}=viewport();
  scrollResultsToTop(host,true,false);
  assert.deepEqual(moves,[{top:0,behavior:'smooth'}]);
  // A smooth transition is owned by the browser, not a synchronous scrollTop change.
  assert.equal(host.scrollTop,960);
});

test('disabling interface animation jumps immediately without smooth scrolling',()=>{
  const {host,moves}=viewport();
  scrollResultsToTop(host,false,false);
  assert.equal(host.scrollTop,0);
  assert.deepEqual(moves,[]);
});

test('system prefers-reduced-motion overrides the in-app animation toggle',()=>{
  const {host,moves}=viewport(450);
  scrollResultsToTop(host,true,true);
  assert.equal(host.scrollTop,0);
  assert.deepEqual(moves,[]);
});

test('missing results viewport is safe during component unmount',()=>{
  assert.doesNotThrow(()=>scrollResultsToTop(null,true,false));
  assert.doesNotThrow(()=>scrollResultsToTop(undefined,false,true));
});
