import { useCallback, useEffect, useRef, useState } from "react";
import type { BangumiSearchPrefill, BangumiSubject, MediaNode, MetadataBinding } from "../types/media";
import { api, isStaleWorkError } from "../lib/api";
import { cleanBangumiKeyword, errorMessage, nodeDisplayTitle } from "../lib/format";
import { Icon } from "./Icon";
import { LoadingState } from "./LoadingState";
import { useI18n } from "../lib/i18n";

interface BangumiModalProps {
  node: MediaNode | null;
  onClose: () => void;
  onStale: () => Promise<void>;
  onBound: (binding: MetadataBinding) => void | Promise<void>;
}

export function BangumiModal({ node, onClose, onBound, onStale }: BangumiModalProps) {
  const { language, t } = useI18n();
  const [keyword, setKeyword] = useState("");
  const [results, setResults] = useState<BangumiSubject[]>([]);
  const [loading, setLoading] = useState(false);
  const [bindingId, setBindingId] = useState<number | null>(null);
  const [searched, setSearched] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [errorKind, setErrorKind] = useState<"search" | "bind">("search");
  const [prefill, setPrefill] = useState<BangumiSearchPrefill | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const requestGenerationRef = useRef(0);
  const activeNodeIdRef = useRef<number | null>(null);

  const requestIsCurrent = useCallback((generation: number, nodeId: number) => (
    requestGenerationRef.current === generation && activeNodeIdRef.current === nodeId
  ), []);

  const close = useCallback(() => {
    requestGenerationRef.current += 1;
    activeNodeIdRef.current = null;
    setLoading(false);
    setBindingId(null);
    onClose();
  }, [onClose]);

  const updateKeyword = useCallback((value: string) => {
    requestGenerationRef.current += 1;
    setKeyword(value);
    setResults([]);
    setError(null);
    setSearched(false);
    setLoading(false);
  }, []);

  useEffect(() => {
    const generation = requestGenerationRef.current + 1;
    requestGenerationRef.current = generation;
    const nodeId = node?.id ?? null;
    activeNodeIdRef.current = nodeId;
    setResults([]);
    setError(null);
    setSearched(false);
    setPrefill(null);
    setLoading(false);
    setBindingId(null);
    setKeyword("");
    if (!node || nodeId == null) return;
    setKeyword(cleanBangumiKeyword(node.displayName || node.folderName));
    void api.bangumiPrefill(node.id).then((value) => {
      if (!requestIsCurrent(generation, nodeId)) return;
      setPrefill(value);
      setKeyword(value.extractedName || value.originalName);
    }).catch(() => undefined);
    const animationFrame = requestAnimationFrame(() => {
      if (requestIsCurrent(generation, nodeId)) inputRef.current?.focus();
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [node?.id, requestIsCurrent]);

  useEffect(() => {
    if (!node) return;
    const key = (event: KeyboardEvent) => event.key === "Escape" && close();
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [close, node]);

  if (!node) return null;

  const search = async (event?: React.FormEvent) => {
    event?.preventDefault();
    const query = keyword.trim();
    if (!query || loading || bindingId !== null) return;
    const nodeId = node.id;
    const generation = requestGenerationRef.current + 1;
    requestGenerationRef.current = generation;
    setLoading(true);
    setSearched(true);
    setError(null);
    setErrorKind("search");
    try {
      const nextResults = await api.searchBangumi(query,20,node.id);
      if (!requestIsCurrent(generation, nodeId)) return;
      setResults(nextResults);
    } catch (caught) {
      if (!requestIsCurrent(generation, nodeId)) return;
      setError(errorMessage(caught));
      setResults([]);
    } finally {
      if (requestIsCurrent(generation, nodeId)) setLoading(false);
    }
  };

  const bind = async (subject: BangumiSubject) => {
    if (bindingId !== null || loading) return;
    const nodeId = node.id;
    const generation = requestGenerationRef.current + 1;
    requestGenerationRef.current = generation;
    setBindingId(subject.subjectId);
    setError(null);
    setErrorKind("bind");
    try {
      const binding = node.workTarget ? await api.bindWorkBangumi(node.workTarget, subject) : await api.bindBangumi(nodeId, subject);
      if (!requestIsCurrent(generation, nodeId)) return;
      await onBound(binding);
      if (!requestIsCurrent(generation, nodeId)) return;
      close();
    } catch (caught) {
      if (!requestIsCurrent(generation, nodeId)) return;
      if (isStaleWorkError(caught)) { await onStale(); close(); return; }
      setError(errorMessage(caught));
    } finally {
      if (requestIsCurrent(generation, nodeId)) setBindingId(null);
    }
  };

  return (
    <div className="modal-backdrop" onMouseDown={(event) => event.target === event.currentTarget && close()} role="presentation">
      <section className="bangumi-modal" role="dialog" aria-modal="true" aria-labelledby="bangumi-title">
        <header className="modal-header">
          <span className="modal-heading-icon"><Icon name="bangumi" /></span>
          <div><p className="eyebrow">{t("bangumi.manualMatch")}</p><h2 id="bangumi-title">{t("bangumi.modalTitle")}</h2><p>{t("bangumi.modalDescription", { name: nodeDisplayTitle(node) })}</p>{node.workTarget && <p>{t("works.wholeGroup", { count: node.workTarget.sourceNodeIds.length })}</p>}</div>
          <button className="modal-close" aria-label={t("common.close")} onClick={close} type="button"><Icon name="close" /></button>
        </header>

        {prefill && <div className="bangumi-keyword-context">
          <div><span>{t("bangumi.originalName")}</span><strong title={prefill.originalName}>{prefill.originalName}</strong></div>
          <div><span>{t("bangumi.extractedName")}</span><button disabled={bindingId !== null} onClick={() => updateKeyword(prefill.extractedName)} type="button">{prefill.extractedName || t("bangumi.noExtracted")}</button></div>
          {prefill.candidates.length > 1 && <div className="keyword-candidates"><span>{t("bangumi.otherCandidates")}</span><p>{prefill.candidates.filter((candidate) => candidate !== prefill.extractedName).map((candidate) => <button disabled={bindingId !== null} key={candidate} onClick={() => updateKeyword(candidate)} type="button">{candidate}</button>)}</p></div>}
        </div>}
        <form className="bangumi-search" onSubmit={search}>
          <Icon name="search" />
          <input ref={inputRef} aria-label={t("bangumi.keywordAria")} disabled={bindingId !== null} onChange={(event) => updateKeyword(event.target.value)} placeholder={t("bangumi.keywordPlaceholder")} value={keyword} />
          {keyword && <button className="clear-input" aria-label={t("common.clear")} disabled={bindingId !== null} onClick={() => updateKeyword("")} type="button"><Icon name="close" /></button>}
          <button className="button primary" disabled={!keyword.trim() || loading || bindingId !== null} type="submit">{loading ? t("common.searching") : t("common.search")}</button>
        </form>

        {error && <div className="inline-error"><Icon name="warning" /><span><strong>{t(errorKind === "search" ? "bangumi.searchFailed" : "bangumi.saveFailed")}</strong><small>{error}</small></span>{errorKind === "search" && <button onClick={() => void search()} type="button">{t("common.retry")}</button>}</div>}
        <div className="bangumi-results">
          {loading && <LoadingState label={t("bangumi.connecting")} />}
          {!loading && !searched && <div className="search-prompt"><Icon name="search" /><strong>{t("bangumi.promptTitle")}</strong><p>{t("bangumi.promptDescription")}</p></div>}
          {!loading && searched && !error && results.length === 0 && <div className="search-prompt"><Icon name="info" /><strong>{t("bangumi.noneTitle")}</strong><p>{t("bangumi.noneDescription")}</p></div>}
          {!loading && results.map((subject) => (
            <article className="bangumi-result" key={subject.subjectId}>
              <span className="bangumi-cover">{subject.imageUrl ? <img alt="" src={subject.imageUrl} /> : <Icon name="image" />}</span>
              <div className="bangumi-result-copy">
                <strong>{(language === "en-US" ? subject.titleEn : language === "ja-JP" ? subject.titleJa : language === "ko-KR" ? subject.titleKo : subject.titleCn) || subject.titleCn || subject.title}</strong>
                {((language === "en-US" ? subject.titleEn : language === "ja-JP" ? subject.titleJa : language === "ko-KR" ? subject.titleKo : subject.titleCn) || subject.titleCn) && <p>{subject.title}</p>}
                <small>{subject.date || t("bangumi.unknownDate")}<span />{t("bangumi.subjectId", { id: subject.subjectId })}</small>
              </div>
              <button className="button secondary" disabled={bindingId !== null} onClick={() => void bind(subject)} type="button">{bindingId === subject.subjectId ? t("bangumi.binding") : t("common.choose")}</button>
            </article>
          ))}
        </div>
        <footer className="modal-footer"><Icon name="shield" />{t("bangumi.footer")}</footer>
      </section>
    </div>
  );
}
