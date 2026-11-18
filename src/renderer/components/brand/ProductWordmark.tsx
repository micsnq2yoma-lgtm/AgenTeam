import mark from '@/assets/brand/agenteam-mark.svg';
import { productName } from '@/assets/brand/brand';

export function ProductWordmark({ name = productName, showMark = true }: {
  name?: string;
  showMark?: boolean;
}) {
  if (name !== productName) return <span>{name}</span>;

  return (
    <span className="agenteam-wordmark" role="img" aria-label={name}>
      {showMark && <img src={mark} alt="" className="agenteam-wordmark-icon" />}
      <span aria-hidden="true">AGEN<span className="agenteam-wordmark-accent">T</span>EAM</span>
    </span>
  );
}
