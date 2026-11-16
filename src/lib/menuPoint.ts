/** Keyboard-activated buttons report (0,0); anchor their menu to the invoker. */
export function menuPoint(event: {clientX: number; clientY: number; currentTarget: EventTarget | null}) {
  if (event.clientX !== 0 || event.clientY !== 0 || !(event.currentTarget instanceof HTMLElement)) {
    return {x:event.clientX,y:event.clientY};
  }
  const bounds=event.currentTarget.getBoundingClientRect();
  return {x:bounds.left,y:bounds.bottom};
}
