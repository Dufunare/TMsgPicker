import type {Post,ScanResult,StopReason,ScanStatus,PostCompletenessLevel} from '../types';

const statusNames:Record<ScanStatus,string>={
  Running:'扫描中', Complete:'扫描范围完成', BudgetExceeded:'资源额度耗尽',
  Interrupted:'扫描中断', Error:'扫描失败',
};
const reasonNames:Record<StopReason,string>={
  PostLimitReached:'已达到要求的 Post 数量',
  DateLowerBoundReached:'已越过日期下界',
  HistoryExhausted:'聊天历史读取完毕',
  UserCancelled:'用户取消',
  CursorStalled:'历史游标未能继续推进',
  ResourceLimitReached:'扫描安全额度已耗尽',
  GatewayFailure:'历史获取或解析失败',
};
const confidenceNames:Record<PostCompletenessLevel,string>={
  Verified:'成员已验证',
  Assumed:'按连续历史推定完整',
  Truncated:'已确认缺少成员',
  Unknown:'成员完整性未知',
};
export const scanStatusLabel=(value:ScanStatus)=>statusNames[value];
export const scanReasonLabel=(value:StopReason|null)=>value?reasonNames[value]:'';
export const postCompletenessLabel=(value:PostCompletenessLevel)=>confidenceNames[value];
export const needsIncompleteConfirmation=(posts:readonly Post[])=>posts.some(p=>
  p.completeness.level==='Unknown'||p.completeness.level==='Truncated');
export const windowPartiallyCovered=(result:ScanResult)=>result.status==='Complete'
  &&result.scope.kind==='dates'&&result.coverage.date_window_exhausted===false;
