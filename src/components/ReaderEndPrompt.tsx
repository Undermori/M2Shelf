import {Icon} from './Icon';
import {useI18n} from '../lib/i18n';
export function ReaderEndPrompt({title,busy,error,onContinue,onDismiss}:{title:string;busy:boolean;error:string;onContinue:()=>void;onDismiss:()=>void}){
 const {t}=useI18n();return <div className="reader-end-prompt"><div><small>{t('nextVolume.end')}</small><button className="button secondary" type="button" disabled={busy} onClick={onContinue}>{t(busy?'nextVolume.opening':'nextVolume.open',{title})}</button>{error&&<span role="alert">{error}</span>}</div><button className="icon-button" type="button" aria-label={t('nextVolume.dismiss')} onClick={onDismiss}><Icon name="close"/></button></div>;
}
