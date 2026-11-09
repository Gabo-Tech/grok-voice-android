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

/// Replace `{name}` style holes in a spoken line.
pub fn fill(locale: &str, key: &str, pairs: &[(&str, &str)]) -> String {
    let mut text = line(locale, key);
    for (hole, value) in pairs {
        text = text.replace(&format!("{{{hole}}}"), value);
    }
    text
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
    ("person-delete-ask", "", "Delete {name} from People? This does not change the phone's Contacts app. Say yes to delete them."),
    ("listen-off-ask", "", "Turn Listen off? Hey Grok stops until you turn it back on. Say yes to turn it off."),
    ("wipe-ask", "", "Wipe pinned facts and recent on-device notes? Notes, People, and voice keys stay. Say yes to wipe them."),
    ("secret-refused", "", "Voice cannot save or read a key or a backup password. Open Settings and add it there."),
    ("keys-refused", "", "Voice cannot reorder or delete saved voice keys. Change that in Settings."),
    ("say-task", "", "Hey, remember to {title}."),
    ("say-task-later", "", "Hey, remember to {title} {when}."),
    ("say-task-short", "", "Hey, remember {title}."),
    ("say-task-short-later", "", "Remember, you have {title} {when}."),
    ("say-event-now", "", "Remember, {title} is starting now."),
    ("say-event-later", "", "Remember, you have {title} {when}."),
    ("say-bday-today", "", "Today is {name}'s birthday."),
    ("say-bday-tomorrow", "", "Hey, remember {name}'s birthday is tomorrow."),
    ("say-bday-later", "", "Hey, remember {name}'s birthday is {when}."),
    ("say-anniv-today", "", "Today is {name}'s anniversary."),
    ("say-anniv-tomorrow", "", "Hey, remember {name}'s anniversary is tomorrow."),
    ("say-anniv-later", "", "Hey, remember {name}'s anniversary is {when}."),
    ("say-alarm", "", "Hey, it's time for {title}."),
    ("say-alarm-wake", "", "Hey, it's time to wake up."),
    ("say-alarm-plain", "", "Hey, an alarm is going off."),
    ("say-timer", "", "Hey, the {title} timer is done."),
    ("say-timer-plain", "", "Hey, your timer is done."),
    ("say-reach", "", "Hey, remember to reach out to {name}."),
    ("say-update", "", "Hey, a new version of Grok Voice is available."),
    ("when-minute", "", "in a minute"),
    ("when-minutes", "", "in {n} minutes"),
    ("when-hour", "", "in an hour"),
    ("when-hours", "", "in {n} hours"),
    ("when-morning", "", "this morning"),
    ("when-afternoon", "", "this afternoon"),
    ("when-evening", "", "this evening"),
    ("when-tonight", "", "tonight"),
    ("when-today", "", "today"),
    ("when-tomorrow", "", "tomorrow"),
    ("when-days", "", "in {n} days"),
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
    ("person-delete-ask", "", "¿Borrar a {name} de Personas? Esto no cambia los contactos del teléfono. Di que sí para borrarlo."),
    ("listen-off-ask", "", "¿Apagar Escuchar? Hey Grok se detiene hasta que lo actives. Di que sí para apagarlo."),
    ("wipe-ask", "", "¿Borrar los datos fijados y las notas recientes del teléfono? Las notas, Personas y las claves de voz se quedan. Di que sí para borrarlos."),
    ("secret-refused", "", "La voz no puede guardar ni leer una clave ni una contraseña de copia. Ábrelo en Ajustes."),
    ("keys-refused", "", "La voz no puede reordenar ni borrar las claves de voz. Cámbialo en Ajustes."),
    ("say-task", "", "Oye, recuerda que tienes que {title}."),
    ("say-task-later", "", "Recuerda que tienes que {title} {when}."),
    ("say-task-short", "", "Oye, recuerda {title}."),
    ("say-task-short-later", "", "Recuerda que tienes {title} {when}."),
    ("say-event-now", "", "Recuerda que {title} empieza ahora."),
    ("say-event-later", "", "Recuerda que tienes {title} {when}."),
    ("say-bday-today", "", "Hoy es el cumpleaños de {name}."),
    ("say-bday-tomorrow", "", "Oye, recuerda que mañana es el cumpleaños de {name}."),
    ("say-bday-later", "", "Oye, recuerda que el cumpleaños de {name} es {when}."),
    ("say-anniv-today", "", "Hoy es el aniversario de {name}."),
    ("say-anniv-tomorrow", "", "Oye, recuerda que mañana es el aniversario de {name}."),
    ("say-anniv-later", "", "Oye, recuerda que el aniversario de {name} es {when}."),
    ("say-alarm", "", "Oye, es hora de {title}."),
    ("say-alarm-wake", "", "Oye, es hora de despertarse."),
    ("say-alarm-plain", "", "Oye, está sonando una alarma."),
    ("say-timer", "", "Oye, el temporizador de {title} ha terminado."),
    ("say-timer-plain", "", "Oye, tu temporizador ha terminado."),
    ("say-reach", "", "Oye, recuerda contactar con {name}."),
    ("say-update", "", "Oye, hay una versión nueva de Grok Voice."),
    ("when-minute", "", "dentro de un minuto"),
    ("when-minutes", "", "dentro de {n} minutos"),
    ("when-hour", "", "dentro de una hora"),
    ("when-hours", "", "dentro de {n} horas"),
    ("when-morning", "", "esta mañana"),
    ("when-afternoon", "", "esta tarde"),
    ("when-evening", "", "esta noche"),
    ("when-tonight", "", "esta noche"),
    ("when-today", "", "hoy"),
    ("when-tomorrow", "", "mañana"),
    ("when-days", "", "dentro de {n} días"),
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
    ("person-delete-ask", "", "{name} aus Personen löschen? Das ändert nicht die Kontakte des Telefons. Sag ja, um zu löschen."),
    ("listen-off-ask", "", "Hören ausschalten? Hey Grok pausiert, bis du es wieder einschaltest. Sag ja zum Ausschalten."),
    ("wipe-ask", "", "Gemerkte Sätze und kurze Notizen der letzten Sitzungen löschen? Notizen, Personen und Sprachschlüssel bleiben. Sag ja zum Löschen."),
    ("secret-refused", "", "Sprache kann keinen Schlüssel und kein Backup-Passwort speichern oder vorlesen. Trag ihn in den Einstellungen ein."),
    ("keys-refused", "", "Sprache kann gespeicherte Sprachschlüssel nicht sortieren oder löschen. Das geht in den Einstellungen."),
    ("say-task", "", "Hey, denk daran, dass du {title} erledigen musst."),
    ("say-task-later", "", "Denk daran, dass du {title} {when} erledigen musst."),
    ("say-task-short", "", "Hey, denk an {title}."),
    ("say-task-short-later", "", "Denk daran, du hast {title} {when}."),
    ("say-event-now", "", "Denk daran, {title} fängt jetzt an."),
    ("say-event-later", "", "Denk daran, du hast {title} {when}."),
    ("say-bday-today", "", "Heute hat {name} Geburtstag."),
    ("say-bday-tomorrow", "", "Hey, denk daran, {name} hat morgen Geburtstag."),
    ("say-bday-later", "", "Hey, denk daran, {name} hat {when} Geburtstag."),
    ("say-anniv-today", "", "Heute ist der Jahrestag von {name}."),
    ("say-anniv-tomorrow", "", "Hey, denk daran, morgen ist der Jahrestag von {name}."),
    ("say-anniv-later", "", "Hey, denk daran, der Jahrestag von {name} ist {when}."),
    ("say-alarm", "", "Hey, es ist Zeit für {title}."),
    ("say-alarm-wake", "", "Hey, es ist Zeit aufzustehen."),
    ("say-alarm-plain", "", "Hey, ein Wecker klingelt."),
    ("say-timer", "", "Hey, der Timer {title} ist fertig."),
    ("say-timer-plain", "", "Hey, dein Timer ist fertig."),
    ("say-reach", "", "Hey, denk daran, dich bei {name} zu melden."),
    ("say-update", "", "Hey, eine neue Version von Grok Voice ist verfügbar."),
    ("when-minute", "", "in einer Minute"),
    ("when-minutes", "", "in {n} Minuten"),
    ("when-hour", "", "in einer Stunde"),
    ("when-hours", "", "in {n} Stunden"),
    ("when-morning", "", "heute Morgen"),
    ("when-afternoon", "", "heute Nachmittag"),
    ("when-evening", "", "heute Abend"),
    ("when-tonight", "", "heute Nacht"),
    ("when-today", "", "heute"),
    ("when-tomorrow", "", "morgen"),
    ("when-days", "", "in {n} Tagen"),
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
    ("person-delete-ask", "", "Apagar {name} de Pessoas? Isto não muda os contatos do telefone. Diga sim para apagar."),
    ("listen-off-ask", "", "Desligar Ouvir? Hey Grok para até você ligar de novo. Diga sim para desligar."),
    ("wipe-ask", "", "Apagar os fatos fixados e as notas recentes do telefone? Notas, Pessoas e chaves de voz ficam. Diga sim para apagar."),
    ("secret-refused", "", "A voz não pode guardar nem ler uma chave ou senha de backup. Adicione em Ajustes."),
    ("keys-refused", "", "A voz não pode reordenar nem apagar as chaves de voz. Mude isso em Ajustes."),
    ("say-task", "", "Ei, lembra que você tem que {title}."),
    ("say-task-later", "", "Lembra que você tem que {title} {when}."),
    ("say-task-short", "", "Ei, lembra de {title}."),
    ("say-task-short-later", "", "Lembra que você tem {title} {when}."),
    ("say-event-now", "", "Lembra que {title} começa agora."),
    ("say-event-later", "", "Lembra que você tem {title} {when}."),
    ("say-bday-today", "", "Hoje é o aniversário de {name}."),
    ("say-bday-tomorrow", "", "Ei, lembra que o aniversário de {name} é amanhã."),
    ("say-bday-later", "", "Ei, lembra que o aniversário de {name} é {when}."),
    ("say-anniv-today", "", "Hoje é o aniversário de casamento de {name}."),
    ("say-anniv-tomorrow", "", "Ei, lembra que o aniversário de casamento de {name} é amanhã."),
    ("say-anniv-later", "", "Ei, lembra que o aniversário de casamento de {name} é {when}."),
    ("say-alarm", "", "Ei, está na hora de {title}."),
    ("say-alarm-wake", "", "Ei, está na hora de acordar."),
    ("say-alarm-plain", "", "Ei, um alarme está tocando."),
    ("say-timer", "", "Ei, o timer de {title} acabou."),
    ("say-timer-plain", "", "Ei, seu timer acabou."),
    ("say-reach", "", "Ei, lembra de falar com {name}."),
    ("say-update", "", "Ei, tem uma versão nova do Grok Voice."),
    ("when-minute", "", "em um minuto"),
    ("when-minutes", "", "em {n} minutos"),
    ("when-hour", "", "em uma hora"),
    ("when-hours", "", "em {n} horas"),
    ("when-morning", "", "hoje de manhã"),
    ("when-afternoon", "", "hoje à tarde"),
    ("when-evening", "", "hoje à noite"),
    ("when-tonight", "", "hoje à noite"),
    ("when-today", "", "hoje"),
    ("when-tomorrow", "", "amanhã"),
    ("when-days", "", "em {n} dias"),
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
    ("person-delete-ask", "", "Esborrar {name} de Persones? Això no canvia els contactes del telèfon. Digues que sí per esborrar-lo."),
    ("listen-off-ask", "", "Apagar Escolta? Hey Grok s'atura fins que ho tornes a activar. Digues que sí per apagar-ho."),
    ("wipe-ask", "", "Esborrar les dades fixades i les notes recents del telèfon? Les notes, Persones i les claus de veu es queden. Digues que sí per esborrar-les."),
    ("secret-refused", "", "La veu no pot guardar ni llegir una clau ni una contrasenya de còpia. Obri Ajustos i afig-la allà."),
    ("keys-refused", "", "La veu no pot reordenar ni esborrar les claus de veu. Canvia-ho en Ajustos."),
    ("say-task", "", "Ei, recorda que has de {title}."),
    ("say-task-later", "", "Recorda que has de {title} {when}."),
    ("say-task-short", "", "Ei, recorda {title}."),
    ("say-task-short-later", "", "Recorda que tens {title} {when}."),
    ("say-event-now", "", "Recorda que {title} comença ara."),
    ("say-event-later", "", "Recorda que tens {title} {when}."),
    ("say-bday-today", "", "Hui és l'aniversari de {name}."),
    ("say-bday-tomorrow", "", "Ei, recorda que l'aniversari de {name} és demà."),
    ("say-bday-later", "", "Ei, recorda que l'aniversari de {name} és {when}."),
    ("say-anniv-today", "", "Hui és la data especial de {name}."),
    ("say-anniv-tomorrow", "", "Ei, recorda que la data especial de {name} és demà."),
    ("say-anniv-later", "", "Ei, recorda que la data especial de {name} és {when}."),
    ("say-alarm", "", "Ei, és l'hora de {title}."),
    ("say-alarm-wake", "", "Ei, és l'hora d'alçar-se."),
    ("say-alarm-plain", "", "Ei, està sonant una alarma."),
    ("say-timer", "", "Ei, el temporitzador de {title} ha acabat."),
    ("say-timer-plain", "", "Ei, el temporitzador ha acabat."),
    ("say-reach", "", "Ei, recorda de parlar amb {name}."),
    ("say-update", "", "Ei, hi ha una versió nova de Grok Voice."),
    ("when-minute", "", "d'ací a un minut"),
    ("when-minutes", "", "d'ací a {n} minuts"),
    ("when-hour", "", "d'ací a una hora"),
    ("when-hours", "", "d'ací a {n} hores"),
    ("when-morning", "", "este matí"),
    ("when-afternoon", "", "esta vesprada"),
    ("when-evening", "", "este vespre"),
    ("when-tonight", "", "esta nit"),
    ("when-today", "", "hui"),
    ("when-tomorrow", "", "demà"),
    ("when-days", "", "d'ací a {n} dies"),
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
        assert!(line("en", "secret-refused").contains("Settings"));
        assert!(line("es", "secret-refused").contains("Ajustes"));
        assert!(line("de", "listen-off-ask").contains("Hören"));
        assert!(line("pt", "keys-refused").contains("Ajustes"));
        assert!(fill("ca-ES-valencia", "person-delete-ask", &[("name", "Ana")]).contains("Ana"));
        assert!(fill("ca-ES-valencia", "listen-off-ask", &[]).contains("s'atura"));
    }
}
