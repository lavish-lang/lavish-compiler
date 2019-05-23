use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while, take_while1},
    character::complete::char,
    combinator::{all_consuming, cut, map, opt},
    error::{context, ParseError},
    multi::{many0, many1, separated_list},
    sequence::{delimited, preceded, separated_pair, terminated, tuple},
    IResult, InputTake,
};

mod errors;
mod span;

use super::ast::*;
pub use errors::*;
pub use span::*;

use std::collections::HashSet;
use std::iter::FromIterator;

pub fn module<E: ParseError<Span>>(i: Span) -> IResult<Span, Module, E> {
    let (i, loc) = loc(i)?;

    all_consuming(terminated(
        map(many0(spaced(nsdecl)), move |namespaces| {
            Module::new(loc.clone(), namespaces)
        }),
        spaced(many0(spaced(comment_line))),
    ))(i)
}

fn spaced<O, E: ParseError<Span>, F>(f: F) -> impl Fn(Span) -> IResult<Span, O, E>
where
    F: Fn(Span) -> IResult<Span, O, E>,
{
    terminated(preceded(sp, f), sp)
}

fn sp<E: ParseError<Span>>(i: Span) -> IResult<Span, Span, E> {
    let chars = " \t\r\n";

    take_while(move |c| chars.contains(c))(i)
}

fn id<E: ParseError<Span>>(i: Span) -> IResult<Span, Identifier, E> {
    let chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_";

    map(take_while1(move |c| chars.contains(c)), |span: Span| {
        let text = span.clone().into();
        Identifier { span, text }
    })(i)
}

fn basetyp<E: ParseError<Span>>(i: Span) -> IResult<Span, Type, E> {
    map(
        spaced(alt((
            map(tag("bool"), |span| (span, BaseType::Bool)),
            map(tag("int32"), |span| (span, BaseType::Int32)),
            map(tag("int64"), |span| (span, BaseType::Int64)),
            map(tag("uint32"), |span| (span, BaseType::UInt32)),
            map(tag("uint64"), |span| (span, BaseType::UInt64)),
            map(tag("float32"), |span| (span, BaseType::Float32)),
            map(tag("float64"), |span| (span, BaseType::Float64)),
            map(tag("string"), |span| (span, BaseType::String)),
            map(tag("bytes"), |span| (span, BaseType::Bytes)),
            map(tag("timestamp"), |span| (span, BaseType::Timestamp)),
        ))),
        |(span, basetyp)| Type {
            span,
            kind: TypeKind::Base(basetyp),
        },
    )(i)
}

fn arraytyp<E: ParseError<Span>>(i: Span) -> IResult<Span, Type, E> {
    let span = i.clone();
    map(
        preceded(
            terminated(spaced(tag("Array")), spaced(char('<'))),
            terminated(typ, spaced(char('>'))),
        ),
        move |t| Type {
            span: span.clone(),
            kind: TypeKind::Array(ArrayType { inner: Box::new(t) }),
        },
    )(i)
}

fn optiontyp<E: ParseError<Span>>(i: Span) -> IResult<Span, Type, E> {
    let span = i.clone();
    map(
        preceded(
            terminated(spaced(tag("Option")), spaced(char('<'))),
            terminated(typ, spaced(char('>'))),
        ),
        move |t| Type {
            span: span.clone(),
            kind: TypeKind::Option(OptionType { inner: Box::new(t) }),
        },
    )(i)
}

fn maptyp<E: ParseError<Span>>(i: Span) -> IResult<Span, Type, E> {
    let span = i.clone();
    map(
        preceded(
            terminated(spaced(tag("Map")), spaced(char('<'))),
            terminated(
                separated_pair(spaced(typ), char(','), spaced(typ)),
                spaced(char('>')),
            ),
        ),
        move |(k, v)| Type {
            span: span.clone(),
            kind: TypeKind::Map(MapType {
                keys: Box::new(k),
                values: Box::new(v),
            }),
        },
    )(i)
}

fn usertyp<E: ParseError<Span>>(i: Span) -> IResult<Span, Type, E> {
    let chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_<>";

    map(take_while1(move |c| chars.contains(c)), |span: Span| Type {
        span,
        kind: TypeKind::User,
    })(i)
}

fn typ<E: ParseError<Span>>(i: Span) -> IResult<Span, Type, E> {
    alt((maptyp, arraytyp, optiontyp, basetyp, usertyp))(i)
}

fn loc<E: ParseError<Span>>(i: Span) -> IResult<Span, Span, E> {
    let o = i.take(0);
    Ok((i, o))
}

fn field<E: ParseError<Span>>(i: Span) -> IResult<Span, Field, E> {
    let (i, comment) = opt(comment)(i)?;
    let (i, loc) = spaced(loc)(i)?;
    let (i, name) = spaced(id)(i)?;
    let ctx = spaced(context(
        "field",
        cut(preceded(spaced(char(':')), spaced(typ))),
    ));

    map(ctx, move |typ| Field {
        comment: comment.clone(),
        name: name.clone(),
        loc: loc.clone(),
        typ,
    })(i)
}

fn fields<E: ParseError<Span>>(i: Span) -> IResult<Span, Vec<Field>, E> {
    terminated(
        separated_list(spaced(char(',')), field),
        opt(spaced(char(','))),
    )(i)
}

fn fnmod<E: ParseError<Span>>(i: Span) -> IResult<Span, FunctionModifier, E> {
    alt((
        map(tag("server"), |_| FunctionModifier::Server),
        map(tag("client"), |_| FunctionModifier::Client),
    ))(i)
}

fn fnmods<E: ParseError<Span>>(i: Span) -> IResult<Span, Vec<FunctionModifier>, E> {
    preceded(sp, separated_list(sp, fnmod))(i)
}

fn results<E: ParseError<Span>>(i: Span) -> IResult<Span, Vec<Field>, E> {
    let (i, _) = spaced(tag("->"))(i)?;

    context(
        "result list",
        cut(delimited(char('('), fields, preceded(sp, char(')')))),
    )(i)
}

fn fndecl<E: ParseError<Span>>(i: Span) -> IResult<Span, FunctionDecl, E> {
    let (i, comment) = opt(comment)(i)?;
    let (i, modifiers) = fnmods(i)?;
    let (i, _) = spaced(tag("fn"))(i)?;
    let (i, loc) = spaced(loc)(i)?;

    context(
        "function declaration",
        cut(map(
            tuple((
                preceded(sp, id),
                preceded(
                    sp,
                    context(
                        "parameter list",
                        delimited(char('('), fields, preceded(sp, char(')'))),
                    ),
                ),
                opt(results),
            )),
            move |(name, params, results)| FunctionDecl {
                loc: loc.clone(),
                comment: comment.clone(),
                modifiers: HashSet::from_iter(modifiers.iter().cloned()),
                name: name.clone(),
                params,
                results: results.unwrap_or_default(),
            },
        )),
    )(i)
}

fn notifdecl<E: ParseError<Span>>(i: Span) -> IResult<Span, FunctionDecl, E> {
    let (i, comment) = opt(comment)(i)?;
    let (i, modifiers) = fnmods(i)?;
    let (i, _) = spaced(tag("nf"))(i)?;
    let (i, loc) = spaced(loc)(i)?;

    context(
        "notification declaration",
        cut(map(
            tuple((
                preceded(sp, id),
                preceded(
                    sp,
                    context(
                        "parameter list",
                        delimited(char('('), fields, preceded(sp, char(')'))),
                    ),
                ),
            )),
            move |(name, params)| FunctionDecl {
                loc: loc.clone(),
                comment: comment.clone(),
                modifiers: {
                    let mut hs = HashSet::from_iter(modifiers.iter().cloned());
                    hs.insert(FunctionModifier::Notification);
                    hs
                },
                name: name.clone(),
                params,
                results: Vec::new(),
            },
        )),
    )(i)
}

fn structdecl<E: ParseError<Span>>(i: Span) -> IResult<Span, StructDecl, E> {
    let (i, comment) = opt(comment)(i)?;
    let (i, _) = preceded(sp, tag("struct"))(i)?;
    let (i, loc) = spaced(loc)(i)?;

    context(
        "struct declaration",
        cut(map(
            tuple((
                preceded(sp, id),
                preceded(sp, delimited(char('{'), fields, preceded(sp, char('}')))),
            )),
            move |(name, fields)| StructDecl {
                loc: loc.clone(),
                comment: comment.clone(),
                name: name.clone(),
                fields,
            },
        )),
    )(i)
}

fn comment_line<E: ParseError<Span>>(i: Span) -> IResult<Span, Span, E> {
    preceded(sp, preceded(tag("//"), preceded(sp, take_until("\n"))))(i)
}

fn comment<E: ParseError<Span>>(i: Span) -> IResult<Span, Comment, E> {
    map(many1(comment_line), |lines| Comment {
        lines: lines.iter().map(|x| x.clone().into()).collect(),
    })(i)
}

fn nsitem<E: ParseError<Span>>(i: Span) -> IResult<Span, NamespaceItem, E> {
    alt((
        map(fndecl, NamespaceItem::Function),
        map(notifdecl, NamespaceItem::Function),
        map(structdecl, NamespaceItem::Struct),
        map(nsdecl, NamespaceItem::Namespace),
    ))(i)
}

fn nsbody<E: ParseError<Span>>(i: Span) -> IResult<Span, Vec<NamespaceItem>, E> {
    terminated(many0(spaced(nsitem)), spaced(many0(comment_line)))(i)
}

fn nsdecl<E: ParseError<Span>>(i: Span) -> IResult<Span, NamespaceDecl, E> {
    let (i, comment) = opt(comment)(i)?;
    let (i, _) = terminated(preceded(sp, tag("namespace")), sp)(i)?;
    let (i, loc) = spaced(loc)(i)?;

    context(
        "namespace declaration",
        cut(map(
            tuple((
                spaced(id),
                delimited(spaced(char('{')), nsbody, spaced(char('}'))),
            )),
            move |(name, items)| NamespaceDecl::new(name, loc.clone(), comment.clone(), items),
        )),
    )(i)
}
