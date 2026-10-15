import {test} from 'node:test';
import assert from 'node:assert/strict';
import {linkedTextParts, safeExternalUrl} from '../src/text/linkedText.ts';

test('Telegram textUrl labels are clickable even without a visible URL',()=>{
  const text='🧭 点击这里，然后访问 https://example.org/plain.';
  const start=text.indexOf('点击这里'); // Includes the emoji's two UTF-16 units.
  const parts=linkedTextParts(text,[{offset:start,length:4,url:'https://example.com/hidden?q=1'}]);
  assert.equal(start,3);
  assert.equal(parts.map(part=>part.text).join(''),text);
  assert.deepEqual(parts.filter(part=>part.url).map(part=>[part.text,part.url]),[
    ['点击这里','https://example.com/hidden?q=1'],
    ['https://example.org/plain','https://example.org/plain']
  ]);
});

test('caption links override visible URL detection without creating duplicate anchors',()=>{
  const text='网站https://t.me/example 查看';
  const parts=linkedTextParts(text,[
    {offset:2,length:'https://t.me/example'.length,url:'https://t.me/example'},
    {offset:text.indexOf('查看'),length:2,url:'https://example.org/details'}
  ]);
  assert.equal(parts.map(part=>part.text).join(''),text);
  assert.equal(parts.filter(part=>part.url).length,2);
  assert.deepEqual(parts.filter(part=>part.url).map(part=>part.text),['https://t.me/example','查看']);
});

test('unsupported schemes and untrusted ranges remain plain text',()=>{
  assert.equal(safeExternalUrl('javascript:alert(1)'),undefined);
  assert.equal(safeExternalUrl('file:///C:/secrets'),undefined);
  assert.equal(safeExternalUrl('tg://resolve?domain=example'),undefined);
  assert.equal(safeExternalUrl('https://example.com/\nspoof'),undefined);
  assert.deepEqual(linkedTextParts('打开这里',[{offset:0,length:4,url:'javascript:alert(1)'}]),[{text:'打开这里'}]);

  const text='😀Hello';
  const parts=linkedTextParts(text,[
    {offset:1,length:2,url:'https://invalid.example'}, // Splits emoji.
    {offset:-1,length:1,url:'https://invalid.example'},
    {offset:2,length:5,url:'https://valid.example'},
    {offset:3,length:2,url:'https://overlap.example'}
  ]);
  assert.equal(parts.map(part=>part.text).join(''),text);
  assert.deepEqual(parts.filter(part=>part.url).map(part=>part.url),['https://valid.example/']);
});

test('legacy messages without TDLib entities still auto-link HTTP(S)',()=>{
  const parts=linkedTextParts('普通 https://example.org/path，结束');
  assert.equal(parts.map(part=>part.text).join(''),'普通 https://example.org/path，结束');
  assert.equal(parts.filter(part=>part.url)[0]?.url,'https://example.org/path');
  assert.deepEqual(linkedTextParts('没有链接'),[{text:'没有链接'}]);
});
