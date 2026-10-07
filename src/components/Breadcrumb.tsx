import type { BreadcrumbItem } from "../types/media";
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";

interface BreadcrumbProps {
  rootLabel: string;
  items: BreadcrumbItem[];
  currentNodeId?: number | null;
  onRoot: () => void;
  onNode: (nodeId: number) => void;
}

export function Breadcrumb({ rootLabel, items, currentNodeId, onRoot, onNode }: BreadcrumbProps) {
  const { t } = useI18n();
  if (items.length === 0) return null;
  return (
    <nav className="breadcrumb" aria-label={t("breadcrumb.location")}>
      <button onClick={onRoot} type="button">{rootLabel}</button>
      {items.map((item) => {
        const current = item.id === currentNodeId;
        return (
          <span className="breadcrumb-segment" key={item.id}>
            <Icon className="breadcrumb-chevron" name="chevron" />
            <button aria-current={current ? "page" : undefined} disabled={current} onClick={() => onNode(item.id)} type="button">{item.displayName}</button>
          </span>
        );
      })}
    </nav>
  );
}
