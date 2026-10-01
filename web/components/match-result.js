import { mountShadow } from "../lib/component.js";

class MatchResult extends HTMLElement {
  constructor() {
    super();
    this.root = mountShadow(this, "/components/match-result.css", `
      <section hidden role="status" aria-live="polite"><div class="result-symbol" aria-hidden="true">✓</div><div class="message"><h2></h2><p></p></div><span class="result-score"></span></section>`);
  }

  set snapshot(snapshot) {
    const section = this.root.querySelector("section");
    const terminal = snapshot && ["finished", "failed"].includes(snapshot.status);
    this.hidden = !terminal;
    section.hidden = !terminal;
    if (!terminal) return;
    const failed = snapshot.status === "failed";
    section.className = failed ? "failed" : "finished";
    const winner = snapshot.state.outcome?.winner;
    const title = failed ? "경기 실행이 중단되었습니다." : winner ? `TEAM ${winner} 승리!` : "이번 경기는 무승부입니다.";
    const reason = snapshot.state.outcome?.reason === "all_delivered" ? "모든 재활용품을 반납했습니다." : `${snapshot.state.max_turns}턴의 경기가 끝났습니다.`;
    this.root.querySelector("h2").textContent = title;
    this.root.querySelector("p").textContent = failed ? snapshot.error || "봇 실행 중 오류가 발생했습니다. 새 경기로 다시 시작할 수 있습니다." : `${reason} 최종 반납 점수로 승패를 판정했습니다.`;
    this.root.querySelector(".result-symbol").textContent = failed ? "!" : winner ? "★" : "=";
    this.root.querySelector(".result-score").textContent = failed ? "실행 오류" : `A ${snapshot.state.scores.A} : ${snapshot.state.scores.B} B`;
  }
}

customElements.define("match-result", MatchResult);
