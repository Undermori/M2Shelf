import {expect,it} from 'vitest';
import {incrementalMessages} from './incrementalMessages';
import {translate,type MessageKey} from './i18n';

it.each(['zh-CN','en-US','ja-JP','ko-KR'] as const)('retains every explicit incremental translation after inherited resources in %s',locale=>{
  for(const [key,value] of Object.entries(incrementalMessages[locale])){
    const sample=value.replace('{count}','3');
    expect(translate(locale,key as MessageKey,{count:3}),`${locale}:${key}`).toBe(sample);
  }
});
