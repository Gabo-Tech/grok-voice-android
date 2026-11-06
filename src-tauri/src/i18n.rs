//! Spoken lines the app itself generates. The model prompt stays English.
//! Locales: en, es, de, pt, and Valencian (`ca-ES-valencia`).

pub fn normalize(tag: &str) -> &'static str {
    let tag = tag.trim().replace('_', "-").to_ascii_lowercase();
    if tag.contains("valencia") {
        return "ca-ES-valencia";
    }
    if tag.starts_with("es") {
        return "es";
    }
    if tag.starts_with("de") {
        return "de";
    }
    if tag.starts_with("pt") {
        return "pt";
    }
    if tag == "ca-es-valencia" {
        return "ca-ES-valencia";
    }
    "en"
}

pub fn line(locale: &str, key: &str) -> String {
    let locale = normalize(locale);
    let table: &[(&str, &str, &str)] = match locale {
        "es" => ES,
        "de" => DE,
        "pt" => PT,
        "ca-ES-valencia" => CA,
        _ => EN,
    };
    table
        .iter()
        .find(|(id, _, _)| *id == key)
        .map(|(_, _, text)| (*text).to_string())
        .unwrap_or_else(|| {
            EN.iter().find(|(id, _, _)| *id == key).map(|(_, _, text)| (*text).to_string()).unwrap_or_else(|| key.to_string())
        })
}

const EN: &[(&str, &str, &str)] = &[
    ("fact-empty", "", "Say the fact to remember."),
    ("fact-missing", "", "That pinned fact is not on this phone."),
    ("fact-full", "", "There are already 10 pinned facts. Forget one before adding another."),
    ("fact-which", "", "Which pinned fact? Say it the way it is stored."),
    ("fact-updated", "", "Updated: {text}"),
    ("fact-remembered", "", "Remembered: {text}"),
    ("fact-forgot", "", "Forgot: {text}"),
    ("budget-near", "", "You've used about {minutes} minutes of today's {budget}-minute voice budget."),
    ("budget-over", "", "You've passed today's soft voice budget of {budget} minutes."),
    ("budget-stop", "", "You've reached today's voice limit of {budget} minutes, so this call stops."),
    ("month-near", "", "You've used about {minutes} minutes of this month's {budget}-minute voice budget."),
    ("month-over", "", "You've passed this month's soft voice budget of {budget} minutes."),
    ("month-stop", "", "You've reached this month's voice limit of {budget} minutes, so this call stops."),
    ("budget-used", "", "Today's voice limit of {budget} minutes is used up."),
    ("handoff-wait", "", "Nothing was opened. Confirm in the app or say yes. Nothing is sent until you confirm in the other app."),
    ("share-note", "", "Share this note"),
    ("share-reminder", "", "Share this reminder"),
    ("share-event", "", "Share this event"),
];

const ES: &[(&str, &str, &str)] = &[
    ("fact-empty", "", "Di el dato que hay que recordar."),
    ("fact-missing", "", "Ese dato fijado no está en este teléfono."),
    ("fact-full", "", "Ya hay 10 datos fijados. Olvida uno antes de añadir otro."),
    ("fact-which", "", "¿Qué dato fijado? Dilo como está guardado."),
    ("fact-updated", "", "Actualizado: {text}"),
    ("fact-remembered", "", "Recordado: {text}"),
    ("fact-forgot", "", "Olvidado: {text}"),
    ("budget-near", "", "Llevas unos {minutes} minutos de los {budget} de hoy."),
    ("budget-over", "", "Has pasado el límite suave de {budget} minutos de hoy."),
    ("budget-stop", "", "Has llegado al límite de {budget} minutos de hoy, así que esta llamada se detiene."),
    ("month-near", "", "Llevas unos {minutes} minutos de los {budget} de este mes."),
    ("month-over", "", "Has pasado el límite suave de {budget} minutos de este mes."),
    ("month-stop", "", "Has llegado al límite de {budget} minutos de este mes, así que esta llamada se detiene."),
    ("budget-used", "", "El límite de {budget} minutos de hoy ya se ha usado."),
    ("handoff-wait", "", "No se ha abierto nada. Confirma en la app o di que sí. No se envía nada hasta que confirmes en la otra app."),
    ("share-note", "", "Compartir esta nota"),
    ("share-reminder", "", "Compartir este recordatorio"),
    ("share-event", "", "Compartir este evento"),
];

const DE: &[(&str, &str, &str)] = &[
    ("fact-empty", "", "Sag den Satz, der gemerkt werden soll."),
    ("fact-missing", "", "Diesen gemerkten Satz gibt es auf diesem Telefon nicht."),
    ("fact-full", "", "Es sind schon 10 gemerkte Sätze. Vergiss einen, bevor du einen neuen hinzufügst."),
    ("fact-which", "", "Welchen gemerkten Satz? Sag ihn so, wie er gespeichert ist."),
    ("fact-updated", "", "Aktualisiert: {text}"),
    ("fact-remembered", "", "Gemerkt: {text}"),
    ("fact-forgot", "", "Vergessen: {text}"),
    ("budget-near", "", "Du hast etwa {minutes} Minuten von den heutigen {budget} Minuten verbraucht."),
    ("budget-over", "", "Du hast das heutige weiche Sprachlimit von {budget} Minuten überschritten."),
    ("budget-stop", "", "Du hast das heutige Sprachlimit von {budget} Minuten erreicht, also endet dieser Anruf."),
    ("month-near", "", "Du hast etwa {minutes} Minuten von den {budget} Minuten dieses Monats verbraucht."),
    ("month-over", "", "Du hast das weiche Monatslimit von {budget} Minuten überschritten."),
    ("month-stop", "", "Du hast das Sprachlimit von {budget} Minuten in diesem Monat erreicht, also endet dieser Anruf."),
    ("budget-used", "", "Das heutige Sprachlimit von {budget} Minuten ist aufgebraucht."),
    ("handoff-wait", "", "Es wurde nichts geöffnet. Bestätige in der App oder sag ja. Gesendet wird erst, wenn du in der anderen App bestätigst."),
    ("share-note", "", "Diese Notiz teilen"),
    ("share-reminder", "", "Diese Erinnerung teilen"),
    ("share-event", "", "Diesen Termin teilen"),
];

const PT: &[(&str, &str, &str)] = &[
    ("fact-empty", "", "Diga o fato para lembrar."),
    ("fact-missing", "", "Esse fato fixado não está neste telefone."),
    ("fact-full", "", "Já há 10 fatos fixados. Esqueça um antes de adicionar outro."),
    ("fact-which", "", "Qual fato fixado? Diga como ele está guardado."),
    ("fact-updated", "", "Atualizado: {text}"),
    ("fact-remembered", "", "Lembrado: {text}"),
    ("fact-forgot", "", "Esquecido: {text}"),
    ("budget-near", "", "Você usou cerca de {minutes} minutos dos {budget} de hoje."),
    ("budget-over", "", "Você passou do limite suave de {budget} minutos de hoje."),
    ("budget-stop", "", "Você chegou ao limite de {budget} minutos de hoje, então esta chamada para."),
    ("month-near", "", "Você usou cerca de {minutes} minutos dos {budget} deste mês."),
    ("month-over", "", "Você passou do limite suave de {budget} minutos deste mês."),
    ("month-stop", "", "Você chegou ao limite de {budget} minutos deste mês, então esta chamada para."),
    ("budget-used", "", "O limite de {budget} minutos de hoje já foi usado."),
    ("handoff-wait", "", "Nada foi aberto. Confirme no app ou diga sim. Nada é enviado até você confirmar no outro app."),
    ("share-note", "", "Compartilhar esta nota"),
    ("share-reminder", "", "Compartilhar este lembrete"),
    ("share-event", "", "Compartilhar este evento"),
];

const CA: &[(&str, &str, &str)] = &[
    ("fact-empty", "", "Digues la dada que cal recordar."),
    ("fact-missing", "", "Eixa dada fixada no està en este telèfon."),
    ("fact-full", "", "Ja hi ha 10 dades fixades. Oblida'n una abans d'afegir-ne una altra."),
    ("fact-which", "", "Quina dada fixada? Digues-la com està guardada."),
    ("fact-updated", "", "Actualitzat: {text}"),
    ("fact-remembered", "", "Recordat: {text}"),
    ("fact-forgot", "", "Oblidat: {text}"),
    ("budget-near", "", "Portes uns {minutes} minuts dels {budget} de hui."),
    ("budget-over", "", "Has passat el límit suau de {budget} minuts de hui."),
    ("budget-stop", "", "Has arribat al límit de {budget} minuts de hui, així que esta crida s'atura."),
    ("month-near", "", "Portes uns {minutes} minuts dels {budget} d'este mes."),
    ("month-over", "", "Has passat el límit suau de {budget} minuts d'este mes."),
    ("month-stop", "", "Has arribat al límit de {budget} minuts d'este mes, així que esta crida s'atura."),
    ("budget-used", "", "El límit de {budget} minuts de hui ja s'ha usat."),
    ("handoff-wait", "", "No s'ha obert res. Confirma en l'app o digues que sí. No s'envia res fins que confirmes en l'altra app."),
    ("share-note", "", "Compartir esta nota"),
    ("share-reminder", "", "Compartir este recordatori"),
    ("share-event", "", "Compartir este esdeveniment"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valencian_tag_is_distinct_from_spanish() {
        assert_eq!(normalize("ca-ES-valencia"), "ca-ES-valencia");
        assert_eq!(normalize("ca_ES_valencia"), "ca-ES-valencia");
        assert_eq!(normalize("es-ES"), "es");
        assert!(line("ca-ES-valencia", "budget-stop").contains("hui"));
        assert!(line("es", "fact-full").contains("10"));
        assert!(!line("en", "handoff-wait").contains("hui"));
    }
}
