import {test} from 'node:test';
import assert from 'node:assert/strict';
import {needsIncompleteConfirmation,postCompletenessLabel,scanStatusLabel,scanReasonLabel,windowPartiallyCovered} from '../src/scan/scanUi.ts';
import type {Post,ScanResult} from '../src/types.ts';

const post=(level:Post['completeness']['level']):Post=>({
  key:'synthetic',kind:'album',members:[],representative:{chat_id:'1',message_id:'1'},
  date:100,text:'',metrics:{views:null,forwards:null,reply_info:null,reactions:null},
  provenance:{},completeness:{level,reason:'synthetic'}
});
const result=(scope:ScanResult['scope']):ScanResult=>({
  run_id:'synthetic',status:'Complete',scope,stop_reason:'PostLimitReached',detail:'',
  coverage:{scope_complete:true,requested_posts:10,examined_posts:10,date_window_exhausted:false},
  stats:{raw_messages_read:25,posts_finalized:10,posts_matched:3,posts_returned:3,pages:2},
  scanned:25,pages:2,exact:true,posts:[]
});

test('scan coverage and album completeness never share one status',()=>{
  assert.equal(scanStatusLabel('Complete'),'扫描范围完成');
  assert.equal(postCompletenessLabel('Assumed'),'按连续历史推定完整');
  assert.equal(scanReasonLabel('PostLimitReached'),'已达到要求的 Post 数量');
  assert.equal(windowPartiallyCovered(result({kind:'dates',from:1,to:101,limit_posts:10})),true);
  assert.equal(windowPartiallyCovered(result({kind:'recent',count:10})),false);
});

test('Unknown and Truncated posts require an explicit forwarding confirmation',()=>{
  assert.equal(needsIncompleteConfirmation([post('Verified'),post('Assumed')]),false);
  assert.equal(needsIncompleteConfirmation([post('Unknown')]),true);
  assert.equal(needsIncompleteConfirmation([post('Truncated')]),true);
});
