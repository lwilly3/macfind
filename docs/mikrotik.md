# Préparer le MikroTik

macfind lit les baux DHCP et la table ARP par l'API REST de RouterOS.
Il faut donc RouterOS **v7.1 ou plus récent**, le service HTTPS `www-ssl`
actif **avec un certificat**, et un compte autorisé à lire.

Toutes les commandes se collent dans **New Terminal** (WinBox) ou en SSH.

## 1. Compte en lecture seule

```routeros
/user group add name=lecture-api policy=read,api,rest-api
/user add name=macfind group=lecture-api password=CHANGE_MOI
```

Un compte existant (par exemple `admin`) fonctionne aussi, mais un compte
dédié ne peut rien modifier sur le routeur si son mot de passe fuit.

## 2. Certificat et service HTTPS

```routeros
/certificate add name=self-signed common-name=192.168.88.1 key-usage=key-cert-sign,crl-sign,tls-server
/certificate sign self-signed
/ip service set [find name=www-ssl] certificate=self-signed disabled=no
```

Remplace `192.168.88.1` par l'adresse de ton routeur.

Vérifie :

```routeros
/ip service print detail where name=www-ssl
```

La ligne doit afficher `certificate=self-signed`. Avec `certificate=none`,
le routeur refuse toutes les connexions HTTPS (voir le dépannage).

À la souris : **IP → Services → www-ssl → Certificate : self-signed → OK**.

## 3. Restreindre l'accès (recommandé)

```routeros
/ip service set [find name=www-ssl] address=192.168.88.0/24
```

Sans cette restriction, le service répond sur toutes les interfaces,
y compris côté Internet si le pare-feu le laisse passer.

## 4. Tester depuis le Mac

```bash
curl -k -u macfind https://192.168.88.1/rest/system/resource
```

Une réponse JSON signifie que tout est prêt.

## Dépannage

| Message | Cause | Solution |
|---|---|---|
| `operation timed out` | Mauvaise adresse dans la config, ou port 443 filtré | Vérifie `url` avec `macfind routers` et le pare-feu du routeur |
| `handshake failure` | `www-ssl` actif sans certificat | Étape 2 : attacher le certificat au service |
| `authentification refusée (401)` | Mauvais utilisateur ou mot de passe | `macfind forget <nom>` puis relancer, ou `--ask` |
| `HTTP 403` | Le groupe du compte n'a pas la policy `rest-api` ou `read` | Corriger le groupe (étape 1) |
| `HTTP 404` sur `/rest/...` | RouterOS v6 (pas d'API REST) | Mettre à jour vers RouterOS v7 |
